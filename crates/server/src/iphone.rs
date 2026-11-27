//! An iPhone on this machine's USB is offered until it is asked for, like a
//! phone on adb's cable. Pair records are kept in the iOS folder of tetherd's
//! data, one per phone, with this host's ids beside them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use ios::lockdown::{self, Description, Pairing};
use ios::phone::Phone;
use ios::usbmux::Mux;
use protocol::{At, Described, DeviceId, DeviceState, InstanceId, Means, Route, Way};
use tokio::task::AbortHandle;

use crate::watch::Sighting;

mod screen;
mod sound;
mod usb;

const ASKING: Duration = Duration::from_secs(1);

pub(crate) struct Iphones(Arc<Shared>);

struct Shared {
    folder: PathBuf,
    /// Its host id and its system id.
    host: (String, String),
    links: Mutex<Links>,
    screens: screen::Screens,
}

#[derive(Default)]
struct Links {
    wanted: HashSet<String>,
    cables: Vec<String>,
    up: HashMap<String, Link>,
    /// The iPhones whose screen capture is switching them in or out of
    /// QuickTime. Each drops off the bus and comes back, which is not an
    /// unplug, and is left alone until it has, so usbmux is opened on the
    /// configuration the capture settled on.
    switching: HashSet<String>,
    /// The sighting each iPhone was last listed as, which a switching one is still
    /// listed as while it is off the bus.
    listed: HashMap<String, Sighting>,
    /// The iPhones put in QuickTime mode since they were asked for, so
    /// their screen starts at once. Each is put in it before usbmux is
    /// opened, which then opens on QuickTime's configuration.
    ready: HashSet<String>,
}

struct Link {
    mux: Mux,
    /// Aborting the task lets go of the phone's USB.
    running: AbortHandle,
    described: Option<Description>,
    state: DeviceState,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.running.abort();
    }
}

fn mention(udid: &str) -> String {
    protocol::mention(&DeviceId::Iphone(udid.to_owned()))
}

/// A random id in the form of a UUID, as lockdown takes host ids.
fn uuid() -> String {
    let mut bytes = [0u8; 16];

    getrandom::fill(&mut bytes).expect("no randomness");

    let hex: String = bytes.iter().map(|byte| format!("{byte:02X}")).collect();

    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

fn host_ids(folder: &Path) -> Result<(String, String)> {
    let path = folder.join("host");

    if let Ok(kept) = std::fs::read_to_string(&path)
        && let Some((host, buid)) = kept.trim().split_once('\n')
    {
        return Ok((host.to_owned(), buid.to_owned()));
    }

    let host = uuid();
    let buid = uuid();

    std::fs::write(&path, format!("{host}\n{buid}\n")).context("ios failed: host ids")?;

    Ok((host, buid))
}

impl Iphones {
    pub(crate) fn new(folder: &Path) -> Result<Self> {
        std::fs::create_dir_all(folder)?;

        let shared = Shared {
            folder: folder.to_owned(),
            host: host_ids(folder)?,
            links: Mutex::default(),
            screens: screen::Screens::default(),
        };

        Ok(Self(Arc::new(shared)))
    }

    pub(crate) fn look(&self) {
        let cables = usb::on_cables();
        let mut guard = self.0.links.lock().unwrap();
        let links = &mut *guard;

        links
            .wanted
            .retain(|udid| cables.contains(udid) || links.switching.contains(udid));
        links.up.retain(|udid, _| cables.contains(udid));
        links.ready.retain(|udid| links.wanted.contains(udid));
        links.cables = cables;

        let reach: Vec<String> = links
            .wanted
            .iter()
            .filter(|udid| !links.up.contains_key(*udid) && !links.switching.contains(*udid))
            .filter(|udid| links.cables.contains(*udid))
            .cloned()
            .collect();
        let (ready, reach): (Vec<String>, Vec<String>) = reach
            .into_iter()
            .partition(|udid| !links.ready.contains(udid));

        for udid in &ready {
            links.switching.insert(udid.clone());
        }

        drop(guard);

        for udid in ready {
            tokio::spawn(self.0.clone().ready(udid));
        }

        for udid in reach {
            self.reach(udid);
        }
    }

    pub(crate) fn want(&self, udid: &str) -> bool {
        let mut links = self.0.links.lock().unwrap();

        if !links.cables.iter().any(|cable| cable == udid) {
            return false;
        }

        links.wanted.insert(udid.to_owned());

        true
    }

    pub(crate) fn let_go(&self, udid: &str) {
        let mut links = self.0.links.lock().unwrap();

        links.wanted.remove(udid);
        links.up.remove(udid);
        links.ready.remove(udid);
        drop(links);
        self.0.screens.end(udid);
        tokio::spawn(self.0.clone().release(udid.to_owned()));
    }

    pub(crate) fn holds(&self, udid: &str) -> bool {
        self.0.links.lock().unwrap().up.contains_key(udid)
    }

    pub(crate) async fn phone(&self, udid: &str) -> Result<Phone<Mux>> {
        let mux = {
            let links = self.0.links.lock().unwrap();
            let link = links
                .up
                .get(udid)
                .with_context(|| format!("ios failed: {} not connected", mention(udid)))?;

            anyhow::ensure!(
                link.state == DeviceState::Ready,
                "ios failed: {} not paired",
                mention(udid)
            );

            link.mux.clone()
        };
        let record = tokio::fs::read(self.0.record_path(udid))
            .await
            .context("ios failed: no pair record")?;

        Ok(Phone::new(mux, record))
    }

    pub(crate) fn sightings(&self, instance: &InstanceId) -> Vec<Sighting> {
        let mut guard = self.0.links.lock().unwrap();
        let links = &mut *guard;
        let cable = |udid: &str| {
            Sighting::route(
                DeviceId::Iphone(udid.to_owned()),
                Route {
                    way: Way {
                        means: Means::Usb,
                        at: At::Instance(instance.clone()),
                    },
                    address: udid.to_owned(),
                    pairing: false,
                },
            )
        };
        let mut seen: Vec<Sighting> = links
            .up
            .iter()
            .map(|(udid, link)| {
                let name = link
                    .described
                    .as_ref()
                    .map(|said| said.name.clone())
                    .unwrap_or_default();
                let described = link.described.as_ref().map(|said| Described {
                    release: said.release.clone(),
                    abis: Vec::new(),
                });

                Sighting {
                    connected: Some(link.state),
                    name,
                    described,
                    ..cable(udid)
                }
            })
            .collect();

        for sighting in &seen {
            links
                .listed
                .insert(sighting.route.address.clone(), sighting.clone());
        }

        // A phone switching before it was ever held, as one being put in
        // QuickTime mode as it is connected, is listed as connecting.
        seen.extend(
            links
                .switching
                .iter()
                .filter(|udid| !links.up.contains_key(*udid))
                .map(|udid| {
                    let listed = links
                        .listed
                        .get(udid)
                        .cloned()
                        .unwrap_or_else(|| cable(udid));

                    Sighting {
                        connected: Some(DeviceState::Connecting),
                        ..listed
                    }
                }),
        );

        seen.extend(
            links
                .cables
                .iter()
                .filter(|udid| !links.up.contains_key(*udid) && !links.switching.contains(*udid))
                .map(|udid| cable(udid)),
        );

        links.listed.retain(|udid, _| links.wanted.contains(udid));

        seen
    }

    fn reach(&self, udid: String) {
        let (mux, running) = match usb::open(&udid) {
            Ok(opened) => opened,
            Err(error) => {
                tracing::warn!("{error:#}");
                self.let_go(&udid);

                return;
            }
        };
        let shared = self.0.clone();
        let gone = udid.clone();
        let running = tokio::spawn(async move {
            running.await;

            let mut links = shared.links.lock().unwrap();

            links.up.remove(&gone);

            // A dropped phone is not taken again by itself. One switching in
            // or out of QuickTime drops off the bus and is taken again.
            if !links.switching.contains(&gone) {
                links.wanted.remove(&gone);
            }
        });
        let link = Link {
            mux: mux.clone(),
            running: running.abort_handle(),
            described: None,
            state: DeviceState::Connecting,
        };

        self.0.links.lock().unwrap().up.insert(udid.clone(), link);

        let shared = self.0.clone();

        tokio::spawn(async move {
            if let Err(error) = shared.pair(&udid, &mux).await {
                tracing::warn!("{udid}: {error:#}");

                let mut links = shared.links.lock().unwrap();

                links.wanted.remove(&udid);
                links.up.remove(&udid);

                return;
            }

            // On a thread of its own, since the cryptexd client's future is
            // not Send: idevice's RsdService is only for a 'static stream,
            // which the compiler cannot see through.
            let runtime = tokio::runtime::Handle::current();

            tokio::task::spawn_blocking(move || runtime.block_on(shared.mount(&udid)));
        });
    }
}

impl Shared {
    /// The phone's developer services such as input need the developer disk
    /// image. Mounting it asks Apple's signing server for it, with the phone's
    /// identifiers, after every restart of the phone.
    async fn mount(self: Arc<Self>, udid: &str) {
        let phone = match Iphones(self.clone()).phone(udid).await {
            Ok(phone) => phone,
            Err(error) => {
                tracing::debug!("{udid}: {error:#}");

                return;
            }
        };

        match ios::ddi::mount(&phone, &self.folder.join("ddi")).await {
            Ok(ios::ddi::Mounted::Now) => tracing::info!("{udid}: developer disk image installed"),
            Ok(ios::ddi::Mounted::Already) => {
                tracing::debug!("{udid}: developer disk image already installed");
            }
            Err(error) => tracing::info!("{udid}: {error:#}"),
        }
    }

    /// A phone that cannot be put in QuickTime mode is reached as it is, and
    /// its capture is started by the first page that shows its screen.
    async fn ready(self: Arc<Self>, udid: String) {
        tracing::info!("{udid}: putting it in QuickTime mode");

        let readied = {
            let turn = self.screens.turn(&udid);
            let _turn = turn.lock().await;

            quicktime::ready(&quicktime::Phone::new(&udid), true).await
        };

        match readied {
            Ok(()) => {
                tracing::info!("{udid}: in QuickTime mode");
                self.screens.start(&self, &udid);
            }
            Err(error) => tracing::warn!("{udid}: {error:#}"),
        }

        let mut links = self.links.lock().unwrap();

        links.ready.insert(udid.clone());
        links.switching.remove(&udid);
    }

    /// Switching an iPhone out of QuickTime mode gives its sound back to its
    /// speaker.
    async fn release(self: Arc<Self>, udid: String) {
        let turn = self.screens.turn(&udid);
        let _turn = turn.lock().await;

        if let Err(error) = quicktime::release(&quicktime::Phone::new(&udid)).await {
            tracing::debug!("{udid}: {error:#}");
        }
    }

    fn paired(&self, udid: &str) -> bool {
        self.links
            .lock()
            .unwrap()
            .up
            .get(udid)
            .is_some_and(|link| link.state == DeviceState::Ready)
    }

    fn switching(&self, udid: &str, switching: bool) {
        let mut links = self.links.lock().unwrap();

        if switching {
            links.switching.insert(udid.to_owned());
        } else {
            links.switching.remove(udid);
        }
    }

    fn record_path(&self, udid: &str) -> PathBuf {
        self.folder.join(format!("{udid}.plist"))
    }

    fn set(&self, udid: &str, change: impl FnOnce(&mut Link)) {
        if let Some(link) = self.links.lock().unwrap().up.get_mut(udid) {
            change(link);
        }
    }

    async fn pair(&self, udid: &str, mux: &Mux) -> Result<()> {
        mux.start().await?;

        let described = lockdown::describe(mux).await?;

        self.set(udid, |link| link.described = Some(described));

        let path = self.record_path(udid);

        if let Ok(record) = tokio::fs::read(&path).await
            && lockdown::session(mux, &record).await.is_ok()
        {
            self.set(udid, |link| link.state = DeviceState::Ready);

            return Ok(());
        }

        self.set(udid, |link| link.state = DeviceState::Unauthorized);

        let (host, buid) = &self.host;
        let name = host_name();

        loop {
            match lockdown::pair(mux, host, buid, &name).await? {
                Pairing::Paired(record) => {
                    tokio::fs::write(&path, record)
                        .await
                        .context("ios failed: saving the pair record")?;
                    self.set(udid, |link| link.state = DeviceState::Ready);

                    return Ok(());
                }
                Pairing::Asking | Pairing::Locked => tokio::time::sleep(ASKING).await,
                Pairing::Refused => anyhow::bail!("pair failed: not trusted on the phone"),
            }
        }
    }
}

/// The trust prompt on the phone shows this name.
fn host_name() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|_| "tetherd".to_owned())
}
