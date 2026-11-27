//! There is one user, and each of their clients is known by the fingerprint of
//! a key pair it holds. A client tetherd does not know waits until a trusted
//! one approves it. With nothing trusted yet, or when started with `--setup`,
//! tetherd logs a setup code, and the first client to give it is trusted.
//!
//! tetherd has its own key pair too, kept in the data folder, and clients
//! recognise it by its fingerprint.

mod keys;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use p256::pkcs8::EncodePublicKey;
use protocol::{Access, Client, Gate, Instance, InstanceId, Topic, Value};

pub(crate) use self::keys::{check, random};
use self::keys::{fingerprint, own_key};
use crate::db::Db;
use crate::topics::Topics;

/// The characters of a setup code, leaving out those easily mistaken for
/// another.
const CODE: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const MISSES: u32 = 5;
const ASKING: usize = 20;

pub(crate) struct Trust {
    db: Arc<Db>,
    topics: Arc<Topics>,
    /// tetherd's own fingerprint, which is also its id as an instance.
    print: String,
    name: Mutex<Option<String>>,
    trusted: Mutex<Vec<Client>>,
    asking: Mutex<Vec<Client>>,
    /// The setup code while there is one, and how many wrong ones were given.
    setup: Mutex<Option<(String, u32)>>,
}

pub(crate) enum Standing {
    Trusted,
    Gate(Gate),
}

impl Trust {
    pub(crate) async fn new(
        db: Arc<Db>,
        data: &Path,
        setup: bool,
        topics: Arc<Topics>,
    ) -> Result<Self> {
        let key = own_key(&data.join("tetherd.key"))?;
        let print = fingerprint(
            key.verifying_key()
                .to_public_key_der()
                .context("tetherd key")?
                .as_bytes(),
        );
        let trusted = db.clients().await?;
        let name = db.instance_name(&print).await?;
        let trust = Self {
            db,
            topics,
            print,
            name: Mutex::new(name),
            setup: Mutex::new(None),
            trusted: Mutex::new(trusted),
            asking: Mutex::default(),
        };

        if setup || trust.trusted.lock().unwrap().is_empty() {
            trust.new_code();
        }

        Ok(trust)
    }

    fn new_code(&self) {
        let code = setup_code();

        // Logged as an error so that no log filter short of `off` hides it.
        tracing::error!("Setup code: {code}");
        *self.setup.lock().unwrap() = Some((code, 0));
    }

    pub(crate) fn primary(&self) -> InstanceId {
        InstanceId(self.print.clone())
    }

    pub(crate) fn access(&self) -> Access {
        Access {
            primary: self.primary(),
            trusted: self.trusted.lock().unwrap().clone(),
            asking: self.asking.lock().unwrap().clone(),
            instances: vec![Instance {
                id: self.primary(),
                name: self.name.lock().unwrap().clone(),
            }],
        }
    }

    fn changed(&self) {
        self.topics
            .value(&Topic::Access, Value::Access(self.access()));
    }

    pub(crate) async fn rename_instance(&self, name: &str) -> Result<String> {
        let name = checked_name(name)?;
        let _writing = self.db.writing().await;

        self.db.name_instance(&self.print, name).await?;

        let was = self.name.lock().unwrap().replace(name.to_owned());

        self.changed();

        Ok(match was {
            None => format!("Named this instance {name}"),
            Some(was) => format!("Renamed {was} to {name}"),
        })
    }

    pub(crate) fn is_trusted(&self, print: &str) -> bool {
        self.trusted
            .lock()
            .unwrap()
            .iter()
            .any(|client| client.print == print)
    }

    /// Outside setup, a client that is not trusted is added to those asking.
    pub(crate) fn standing(&self, client: &Client, missed: bool) -> Standing {
        if self.is_trusted(&client.print) {
            return Standing::Trusted;
        }

        if self.setup.lock().unwrap().is_some() {
            return Standing::Gate(Gate::Setup {
                primary: self.print.clone(),
                you: client.print.clone(),
                missed,
            });
        }

        let mut asking = self.asking.lock().unwrap();

        if !asking.iter().any(|one| one.print == client.print) {
            // Anyone who reaches the port can ask, so the oldest are dropped.
            if asking.len() >= ASKING {
                asking.remove(0);
            }

            asking.push(Client {
                at: now(),
                ..client.clone()
            });
            drop(asking);
            self.changed();
        }

        Standing::Gate(Gate::Waiting {
            you: client.print.clone(),
        })
    }

    /// `knows` is handed the code and checks the client's proof against it, so
    /// the code itself is never sent.
    pub(crate) async fn claim(
        &self,
        client: &Client,
        knows: impl FnOnce(&str) -> bool,
    ) -> Result<bool> {
        {
            let mut setup = self.setup.lock().unwrap();
            let Some((wanted, misses)) = setup.as_mut() else {
                return Ok(false);
            };

            if !knows(wanted) {
                *misses += 1;

                // A new code every few misses stops guessing.
                if *misses >= MISSES {
                    drop(setup);
                    self.new_code();
                }

                return Ok(false);
            }

            *setup = None;
        }

        self.trust(client.clone()).await?;

        Ok(true)
    }

    async fn trust(&self, client: Client) -> Result<()> {
        let client = Client {
            at: now(),
            ..client
        };
        let _writing = self.db.writing().await;

        self.db.trust(&client).await?;
        self.asking
            .lock()
            .unwrap()
            .retain(|one| one.print != client.print);

        {
            let mut trusted = self.trusted.lock().unwrap();

            trusted.retain(|one| one.print != client.print);
            trusted.push(client);
        }

        self.changed();

        Ok(())
    }

    pub(crate) async fn approve(&self, print: &str) -> Result<String> {
        let client = self
            .asking
            .lock()
            .unwrap()
            .iter()
            .find(|one| one.print == print)
            .cloned()
            .context("not waiting")?;
        let name = client.name.clone();

        self.trust(client).await?;

        Ok(format!("Approved {name}"))
    }

    pub(crate) fn reject(&self, print: &str) -> Result<String> {
        let rejected = {
            let mut asking = self.asking.lock().unwrap();
            let at = asking
                .iter()
                .position(|one| one.print == print)
                .context("not waiting")?;

            asking.remove(at)
        };

        self.changed();

        Ok(format!("Rejected {}", rejected.name))
    }

    pub(crate) async fn revoke(&self, print: &str) -> Result<String> {
        // Held until both the database and the list have dropped the client,
        // so that `used` cannot save it again in between.
        let _writing = self.db.writing().await;
        let name = self
            .trusted
            .lock()
            .unwrap()
            .iter()
            .find(|one| one.print == print)
            .map(|one| one.name.clone())
            .context("not trusted")?;

        self.db.distrust(print).await?;

        let mut trusted = self.trusted.lock().unwrap();

        trusted.retain(|one| one.print != print);

        let none = trusted.is_empty();

        drop(trusted);

        // With no one left to approve others, only a setup code lets anyone in.
        if none {
            self.new_code();
        }

        self.changed();

        Ok(format!("Revoked {name}"))
    }

    /// The name a client gives itself when it connects again does not replace
    /// this one.
    pub(crate) async fn rename(&self, print: &str, name: &str) -> Result<String> {
        let name = checked_name(name)?;
        let _writing = self.db.writing().await;
        let (was, client) = {
            let mut trusted = self.trusted.lock().unwrap();
            let client = trusted
                .iter_mut()
                .find(|one| one.print == print)
                .context("not trusted")?;
            let was = std::mem::replace(&mut client.name, name.to_owned());

            (was, client.clone())
        };

        self.db.trust(&client).await?;
        self.changed();

        Ok(format!("Renamed {was} to {name}"))
    }

    pub(crate) async fn used(&self, print: &str) {
        let _writing = self.db.writing().await;
        let client = {
            let mut trusted = self.trusted.lock().unwrap();

            let Some(client) = trusted.iter_mut().find(|one| one.print == print) else {
                return;
            };

            client.at = now();
            client.clone()
        };

        if let Err(error) = self.db.trust(&client).await {
            tracing::error!("db failed: {error:#}");
        }

        self.changed();
    }
}

pub(crate) fn checked_name(name: &str) -> Result<&str> {
    let name = name.trim();

    ensure!(
        !name.is_empty() && name.chars().count() <= 64 && !name.contains(char::is_control),
        "invalid name"
    );

    Ok(name)
}

fn setup_code() -> String {
    // Bytes at or above `even` are skipped so that every character is
    // equally likely.
    let even = 256 - 256 % CODE.len();
    let mut code = String::new();

    while code.len() < 6 {
        for byte in random::<16>() {
            if usize::from(byte) < even && code.len() < 6 {
                code.push(char::from(CODE[usize::from(byte) % CODE.len()]));
            }
        }
    }

    code
}

pub(crate) fn host_name() -> String {
    std::process::Command::new("uname")
        .arg("-n")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "tetherd".to_owned())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use p256::ecdsa::SigningKey;

    use super::keys::sign;
    use super::*;

    fn client(key: &SigningKey) -> Client {
        let (public, _) = sign(key, b"").unwrap();

        Client {
            print: fingerprint(&public),
            name: "Firefox on Linux".to_owned(),
            kind: "Browser".to_owned(),
            at: 0,
        }
    }

    #[tokio::test]
    async fn setup_then_approval() {
        let folder = std::env::temp_dir().join(format!("tetherd-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let trust = Trust::new(
            Arc::new(Db::in_memory().await),
            &folder,
            false,
            Arc::default(),
        )
        .await
        .unwrap();
        let first = client(&SigningKey::from_slice(&[1; 32]).unwrap());
        let second = client(&SigningKey::from_slice(&[2; 32]).unwrap());

        assert!(matches!(
            trust.standing(&first, false),
            Standing::Gate(Gate::Setup { .. })
        ));

        // The page proves the code for this nonce and offer without sending
        // it, and the code's case does not matter.
        let nonce = b"nonce";
        let offer = "v=0";
        let shows = |code: &str| {
            let proof = protocol::code_proof(code, nonce, offer);

            move |wanted: &str| protocol::code_proof(wanted, nonce, offer) == proof
        };

        assert!(!trust.claim(&first, shows("WRONG1")).await.unwrap());
        let code = trust.setup.lock().unwrap().clone().unwrap().0;

        assert!(
            trust
                .claim(&first, shows(&code.to_lowercase()))
                .await
                .unwrap()
        );
        assert!(matches!(trust.standing(&first, false), Standing::Trusted));

        assert!(matches!(
            trust.standing(&second, false),
            Standing::Gate(Gate::Waiting { .. })
        ));
        assert!(!trust.claim(&second, shows(&code)).await.unwrap());
        trust.approve(&second.print).await.unwrap();
        assert!(trust.is_trusted(&second.print));

        assert!(trust.rename(&second.print, "  ").await.is_err());
        trust.rename(&second.print, " Laptop ").await.unwrap();
        trust.used(&second.print).await;
        assert_eq!(trust.db.clients().await.unwrap()[1].name, "Laptop");

        trust.revoke(&second.print).await.unwrap();
        assert!(!trust.is_trusted(&second.print));
        assert!(trust.setup.lock().unwrap().is_none());

        trust.revoke(&first.print).await.unwrap();
        assert!(matches!(
            trust.standing(&first, false),
            Standing::Gate(Gate::Setup { .. })
        ));

        let db = Arc::new(Db::in_memory().await);
        let again = Trust::new(db.clone(), &folder, false, Arc::default())
            .await
            .unwrap();

        assert_eq!(again.print, trust.print);

        assert_eq!(
            again.rename_instance(" Desk ").await.unwrap(),
            "Named this instance Desk"
        );

        let again = Trust::new(db, &folder, false, Arc::default())
            .await
            .unwrap();

        assert_eq!(again.access().instances[0].name.as_deref(), Some("Desk"));
        assert_eq!(again.access().primary, again.primary());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn setup_codes_are_six_of_the_characters() {
        let code = setup_code();

        assert_eq!(code.len(), 6);
        assert!(code.bytes().all(|byte| CODE.contains(&byte)));
    }
}
