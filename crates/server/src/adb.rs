//! tetherd speaks the adb protocol to each device's adbd itself, without an adb
//! server or the adb program. A device is connected to only when it is asked
//! for, and is never connected to again by itself.
//!
//! tetherd's key is `adbkey` in the Android folder of its data, where the
//! emulators it starts find and trust it.

pub(crate) mod host;
mod screen;

pub(crate) use screen::Screens;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use android::props::Description;
use anyhow::{Context, Result, bail};
use protocol::{At, Described, DeviceId, DeviceState, InstanceId, Means, Route, Way};
use tokio::io::DuplexStream;
use tokio::net::TcpStream;
use tokio::task::AbortHandle;
use tokio::time::timeout;

use self::host::{Key, Offer, Reader, Session};
use crate::watch::Sighting;

/// The console ports emulators take by default, as the adb program scans them.
const EMULATOR_PORTS: std::ops::RangeInclusive<u16> = 5554..=5584;

/// A phone waits for its owner to allow a connection, which can take a while.
const ALLOWING: Duration = Duration::from_secs(120);

const ANSWERING: Duration = Duration::from_secs(5);

struct Link {
    device: adb::Device<Session>,
    reader: AbortHandle,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

#[derive(Default)]
struct Links {
    /// Keyed by address: `emulator-5554`, a serial number on a cable, or
    /// `host:port` on the network.
    up: HashMap<String, Arc<Link>>,
    connecting: HashSet<String>,
    asking: HashSet<String>,
    failed: HashMap<String, String>,
}

struct Shared {
    key: Arc<Key>,
    listening: Option<host::Listening>,
    links: Mutex<Links>,
    emulators: Mutex<Vec<u16>>,
    others: Mutex<Vec<u16>>,
    /// Phones on a cable are only connected to when asked, so that a browser on
    /// this machine can take one over WebUSB instead.
    cables: Mutex<Vec<String>>,
    known: Mutex<HashMap<String, Description>>,
    /// Kept after a device stops answering, so that it is still listed as
    /// itself.
    ids: Mutex<HashMap<String, String>>,
    routes: RwLock<HashMap<String, String>>,
}

#[derive(Clone)]
pub(crate) struct Adb(Arc<Shared>);

pub(crate) fn over_network(address: &str) -> bool {
    address.contains(':') || address.contains("._tcp")
}

pub(crate) fn means_of(address: &str) -> Means {
    if address.starts_with("emulator-") {
        Means::Emulator
    } else if over_network(address) {
        Means::Wifi
    } else {
        Means::Usb
    }
}

pub(crate) fn id_for(serial: &str, instance: &InstanceId) -> DeviceId {
    match serial
        .strip_prefix("emulator-")
        .and_then(|port| port.parse().ok())
    {
        Some(port) => DeviceId::Emulator {
            port,
            instance: instance.clone(),
        },
        None => DeviceId::Android(serial.to_owned()),
    }
}

fn uri_of(address: &str) -> String {
    if over_network(address) {
        format!("tcp://{address}")
    } else {
        format!("usb://serial/{address}")
    }
}

/// A phone's network name is `adb-39181FDJG005SD-4EMMEg`, with or without the
/// service kind after it. The part after the serial number changes, so only the
/// number identifies the phone.
pub(crate) fn serial_in(name: &str) -> Option<&str> {
    let name = name.strip_prefix("adb-")?.split("._").next()?;
    let (serial, _) = name.rsplit_once('-')?;

    (!serial.is_empty()).then_some(serial)
}

/// The error libadb gives for an unpaired phone tells the user to run the adb
/// program, which tetherd never runs, so it is replaced.
fn connect_failure(error: &anyhow::Error) -> String {
    let message = format!("{error:#}");

    if message.contains("does not trust this key") {
        "adb failed: not paired. Pair it with a pairing code.".to_owned()
    } else {
        message
    }
}

pub(crate) async fn output(command: &mut tokio::process::Command) -> Result<String> {
    let output = command.stdin(std::process::Stdio::null()).output().await?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let said = if stderr.trim().is_empty() {
            &stdout
        } else {
            &stderr
        };

        bail!("{}", said.trim());
    }

    Ok(stdout.into_owned())
}

// shortcut: only phones with wireless debugging, add `_adb._tcp` if one
// that was put in TCP mode over a cable has to be found too.
fn discovered(nearby: &[host::Nearby], instance: &InstanceId) -> Vec<Sighting> {
    let mut found = Vec::new();

    for offer in [Offer::Pairing, Offer::Connecting] {
        for phone in nearby.iter().filter(|phone| phone.offer == offer) {
            let Some(serial) = serial_in(&phone.name) else {
                continue;
            };

            found.push(Sighting::route(
                DeviceId::Android(serial.to_owned()),
                Route {
                    way: Way {
                        means: Means::Wifi,
                        at: At::Instance(instance.clone()),
                    },
                    address: phone.address.clone(),
                    pairing: offer == Offer::Pairing,
                },
            ));
        }
    }

    found
}

impl Adb {
    pub(crate) fn new(android: &Path) -> Result<Self> {
        let name = format!("{}@tetherd", std::env::var("USER").unwrap_or_default());

        Ok(Adb(Arc::new(Shared {
            key: Key::load_or_generate(android, &name)?,
            listening: host::Listening::start()
                .inspect_err(|error| tracing::warn!("mdns failed: {error:#}"))
                .ok(),
            links: Mutex::default(),
            emulators: Mutex::default(),
            others: Mutex::default(),
            cables: Mutex::default(),
            known: Mutex::default(),
            ids: Mutex::default(),
            routes: RwLock::default(),
        })))
    }

    pub(crate) fn set_emulators(&self, ports: Vec<u16>) {
        *self.0.emulators.lock().unwrap() = ports;
    }

    pub(crate) fn route(&self, serial: &str) -> String {
        let routes = self.0.routes.read().unwrap();

        routes
            .get(serial)
            .cloned()
            .unwrap_or_else(|| serial.to_owned())
    }

    pub(crate) fn device(&self, serial: &str) -> Result<adb::Device<Session>> {
        self.at(&self.route(serial))
    }

    fn at(&self, address: &str) -> Result<adb::Device<Session>> {
        let links = self.0.links.lock().unwrap();
        let link = links
            .up
            .get(address)
            .with_context(|| format!("adb failed: {address} not connected"))?;

        Ok(link.device.clone())
    }

    pub(crate) async fn install(&self, serial: &str, apk: &[u8]) -> Result<()> {
        // A name of its own for each install lets two run on one device at once.
        static INSTALLS: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "tetherd-{}-{}",
            std::process::id(),
            INSTALLS.fetch_add(1, Ordering::Relaxed)
        );

        android::apps::install(&self.device(serial)?, apk, &name).await
    }

    pub(crate) async fn open(&self, serial: &str, service: &str) -> Result<DuplexStream> {
        Ok(host::stream(self.device(serial)?.channel(service).await?))
    }

    pub(crate) async fn emu(&self, serial: &str, said: &str) -> Result<String> {
        let port = serial
            .strip_prefix("emulator-")
            .and_then(|port| port.parse().ok())
            .with_context(|| format!("not an emulator: {serial}"))?;

        // Whatever listens on that port may not be a console that answers.
        timeout(ANSWERING, host::console(port, said))
            .await
            .with_context(|| format!("console failed: {serial} did not answer"))?
    }

    fn reach(&self, address: &str, uri: String) {
        {
            let mut links = self.0.links.lock().unwrap();

            if links.up.contains_key(address) || !links.connecting.insert(address.to_owned()) {
                return;
            }

            links.failed.remove(address);
        }

        let adb = self.clone();
        let address = address.to_owned();

        tokio::spawn(async move {
            let shared = adb.0.clone();
            let asked = address.clone();
            let asking = move || {
                shared.links.lock().unwrap().asking.insert(asked.clone());
            };
            let connected = timeout(ALLOWING, host::connect(&uri, &adb.0.key, asking)).await;
            let mut links = adb.0.links.lock().unwrap();
            // A device let go of while it was being connected to stays let go
            // of.
            let wanted = links.connecting.remove(&address);

            links.asking.remove(&address);

            let (device, reader) = match connected {
                Ok(Ok(connected)) => connected,
                Ok(Err(error)) => {
                    links.failed.insert(address, connect_failure(&error));
                    return;
                }
                Err(_) => {
                    links
                        .failed
                        .insert(address, "adb failed: not allowed in time".to_owned());
                    return;
                }
            };

            if !wanted {
                return;
            }

            let link = adb.link(&address, device, reader);

            links.failed.remove(&address);
            links.up.insert(address, Arc::new(link));
        });
    }

    fn link(&self, address: &str, device: adb::Device<Session>, reader: Reader) -> Link {
        let gone = self.clone();
        let lost = address.to_owned();
        let reading = tokio::spawn(async move {
            let _ = reader.run().await;

            gone.0.links.lock().unwrap().up.remove(&lost);
            gone.0.known.lock().unwrap().remove(&lost);
        });

        Link {
            device,
            reader: reading.abort_handle(),
        }
    }

    pub(crate) async fn connect(&self, address: &str, name: &str) -> Result<String> {
        let uri = match address
            .strip_prefix("emulator-")
            .and_then(|port| port.parse::<u16>().ok())
        {
            Some(console) => format!("tcp://127.0.0.1:{}", console + 1),
            None => uri_of(address),
        };

        self.reach(address, uri);

        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let links = self.0.links.lock().unwrap();

            if links.up.contains_key(address) {
                return Ok(format!("Connected to {name}"));
            }

            if let Some(why) = links.failed.get(address) {
                bail!("{why}");
            }

            if links.asking.contains(address) {
                return Ok(format!("Allow this computer on {name}"));
            }
        }

        Ok(format!("Connecting to {name}"))
    }

    pub(crate) fn nearby(&self) -> Vec<host::Nearby> {
        self.0
            .listening
            .as_ref()
            .map(host::Listening::nearby)
            .unwrap_or_default()
    }

    pub(crate) async fn pair(&self, address: &str, code: &str) -> Result<()> {
        self.0.key.pair(address, code).await
    }

    pub(crate) fn let_go(&self, address: &str) {
        let mut links = self.0.links.lock().unwrap();

        links.up.remove(address);
        links.connecting.remove(address);
        links.asking.remove(address);
        links.failed.remove(address);
    }

    pub(crate) fn addresses_of(&self, serial: &str) -> Vec<String> {
        let addresses: Vec<String> = {
            let links = self.0.links.lock().unwrap();

            links.up.keys().chain(&links.connecting).cloned().collect()
        };

        addresses
            .into_iter()
            .filter(|address| self.id_of(address) == serial)
            .collect()
    }

    fn id_of(&self, address: &str) -> String {
        let described = self
            .0
            .known
            .lock()
            .unwrap()
            .get(address)
            .map(|known| known.serial.clone());
        let said = described.or_else(|| self.0.ids.lock().unwrap().get(address).cloned());

        said.or_else(|| serial_in(address).map(str::to_owned))
            .unwrap_or_else(|| address.to_owned())
    }

    async fn look(&self) {
        let mut consoles: BTreeSet<u16> = EMULATOR_PORTS.step_by(2).collect();
        let ours = self.0.emulators.lock().unwrap().clone();
        let mut others = Vec::new();

        consoles.extend(ours.iter().copied());

        for console in consoles {
            let address = format!("emulator-{console}");

            if self.connected(&address) {
                continue;
            }

            let port = console + 1;
            let listening = timeout(
                Duration::from_millis(200),
                TcpStream::connect(("127.0.0.1", port)),
            )
            .await
            .is_ok_and(|stream| stream.is_ok());

            if !listening {
                continue;
            }

            // Starting one of tetherd's emulators is what asks for it.
            if ours.contains(&console) {
                self.reach(&address, format!("tcp://127.0.0.1:{port}"));
            } else {
                others.push(console);
            }
        }

        *self.0.others.lock().unwrap() = others;
        *self.0.cables.lock().unwrap() = host::on_cables();
    }

    /// Android's adbd is up early in a boot and goes at once when Android shuts
    /// down, before the device list is refreshed.
    pub(crate) fn connected(&self, address: &str) -> bool {
        self.0.links.lock().unwrap().up.contains_key(address)
    }

    pub(crate) async fn sightings(&self, instance: &InstanceId) -> Vec<Sighting> {
        self.look().await;

        let (up, connecting, asking) = {
            let links = self.0.links.lock().unwrap();
            let up: Vec<String> = links.up.keys().cloned().collect();
            let asking: Vec<String> = links.asking.iter().cloned().collect();
            let connecting: Vec<String> = links
                .connecting
                .iter()
                .filter(|address| !links.asking.contains(*address))
                .cloned()
                .collect();

            (up, connecting, asking)
        };

        self.0
            .known
            .lock()
            .unwrap()
            .retain(|address, _| up.contains(address));

        let mut seen = Vec::new();
        let mut routes = HashMap::new();

        for address in &up {
            let described = self.describe(address).await;
            let serial = described
                .as_ref()
                .map_or_else(|| self.id_of(address), |said| said.serial.clone());
            // One still booting is up and does not answer yet.
            let state = if described.is_some() {
                DeviceState::Ready
            } else {
                DeviceState::Connecting
            };
            let name = described
                .as_ref()
                .map(|said| said.model.clone())
                .unwrap_or_default();
            let described = described.map(|said| Described {
                release: said.release,
                abis: said.abis,
            });

            routes.insert(serial.clone(), address.clone());
            seen.push(Sighting {
                name,
                described,
                ..self.sighting(&serial, address, state, instance)
            });
        }

        *self.0.routes.write().unwrap() = routes;

        for address in &asking {
            seen.push(self.sighting(
                &self.id_of(address),
                address,
                DeviceState::Unauthorized,
                instance,
            ));
        }

        for address in &connecting {
            seen.push(self.sighting(
                &self.id_of(address),
                address,
                DeviceState::Connecting,
                instance,
            ));
        }

        let cables = self.0.cables.lock().unwrap().clone();
        let others = self.0.others.lock().unwrap().clone();
        let place = At::Instance(instance.clone());

        seen.extend(cables.into_iter().map(|serial| {
            Sighting::route(
                DeviceId::Android(serial.clone()),
                Route {
                    way: Way {
                        means: Means::Usb,
                        at: place.clone(),
                    },
                    address: serial,
                    pairing: false,
                },
            )
        }));
        seen.extend(discovered(&self.nearby(), instance));
        seen.extend(others.into_iter().map(|console| {
            let address = format!("emulator-{console}");

            Sighting::route(
                id_for(&address, instance),
                Route {
                    way: Way {
                        means: Means::Emulator,
                        at: place.clone(),
                    },
                    address,
                    pairing: false,
                },
            )
        }));

        seen
    }

    fn sighting(
        &self,
        serial: &str,
        address: &str,
        state: DeviceState,
        instance: &InstanceId,
    ) -> Sighting {
        Sighting {
            connected: Some(state),
            ..Sighting::route(
                id_for(serial, instance),
                Route {
                    way: Way {
                        means: means_of(address),
                        at: At::Instance(instance.clone()),
                    },
                    address: address.to_owned(),
                    pairing: false,
                },
            )
        }
    }

    async fn describe(&self, address: &str) -> Option<Description> {
        let known = self.0.known.lock().unwrap().get(address).cloned();

        if known.is_some() {
            return known;
        }

        let device = self.at(address).ok()?;

        // A phone on the network can vanish silently, and then a question to it
        // never ends. Its connection is dropped so it does not hold up every
        // device after it.
        let Ok(said) = timeout(ANSWERING, android::props::describe(&device)).await else {
            tracing::warn!("adb failed: {address} did not answer in {ANSWERING:?}");
            self.0.links.lock().unwrap().up.remove(address);

            return None;
        };

        let mut said = said?;

        // A phone is the same phone by cable and over the network, and its
        // serial number says which it is. An emulator goes by its address.
        if address.starts_with("emulator-") || said.serial.is_empty() {
            said.serial = self.id_of(address);
        }

        if over_network(address) {
            self.0
                .ids
                .lock()
                .unwrap()
                .insert(address.to_owned(), said.serial.clone());
        }

        self.0
            .known
            .lock()
            .unwrap()
            .insert(address.to_owned(), said.clone());

        Some(said)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_serial_number_in_a_name_on_the_network() {
        assert_eq!(
            serial_in("adb-39181FDJG005SD-4EMMEg"),
            Some("39181FDJG005SD")
        );
        assert_eq!(
            serial_in("adb-39181FDJG005SD-4EMMEg._adb-tls-connect._tcp"),
            Some("39181FDJG005SD")
        );
        assert_eq!(serial_in("adb-EMULATOR37X3X9X0"), None);
        assert_eq!(serial_in("192.168.1.24:37215"), None);
        assert_eq!(serial_in("39181FDJG005SD"), None);
    }

    #[test]
    fn reads_the_phones_on_the_network_as_routes() {
        let here = InstanceId("SHA256:here".to_owned());
        let nearby = |name: &str, offer, address: &str| host::Nearby {
            name: name.to_owned(),
            offer,
            address: address.to_owned(),
        };
        let services = [
            nearby("adb-AAA-1", Offer::Connecting, "192.168.1.24:37215"),
            nearby("adb-BBB-2", Offer::Connecting, "192.168.1.30:40001"),
            nearby("adb-BBB-2", Offer::Pairing, "192.168.1.30:46693"),
            nearby("unnamed", Offer::Connecting, "192.168.1.31:40001"),
        ];
        let found: Vec<(DeviceId, String, bool)> = discovered(&services, &here)
            .into_iter()
            .map(|seen| (seen.id, seen.route.address, seen.route.pairing))
            .collect();

        assert_eq!(
            found,
            [
                (
                    DeviceId::Android("BBB".to_owned()),
                    "192.168.1.30:46693".to_owned(),
                    true
                ),
                (
                    DeviceId::Android("AAA".to_owned()),
                    "192.168.1.24:37215".to_owned(),
                    false
                ),
                (
                    DeviceId::Android("BBB".to_owned()),
                    "192.168.1.30:40001".to_owned(),
                    false
                ),
            ]
        );
    }

    #[test]
    fn tells_what_an_address_is_reached_by() {
        let here = InstanceId("SHA256:here".to_owned());

        assert_eq!(means_of("emulator-5554"), Means::Emulator);
        assert_eq!(means_of("10.0.0.2:5555"), Means::Wifi);
        assert_eq!(means_of("39181FDJG005SD"), Means::Usb);
        assert_eq!(
            id_for("emulator-5556", &here),
            DeviceId::Emulator {
                port: 5556,
                instance: here.clone()
            }
        );

        assert_eq!(id_for("AAA", &here), DeviceId::Android("AAA".to_owned()));
    }
}
