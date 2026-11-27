use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use libadb::keys::rsa::rand_core::OsRng;
use libadb::keys::{AdbKey, KeyName, store};
use libadb::transport::any;
use libadb::transport::nusb::Nusb;
use libadb::transport::runtime::{Runtime, Tokio};
use libadb::{Connection, ConnectionConfig, HostBanner, TlsClientConfig, TlsIdentity};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::net::TcpStream;

use adb::{Channel, Device, failed};

type Wire = libadb::transport::common::Transport<
    libadb::transport::tls::MaybeTls<<Tokio as Runtime>::Tcp>,
    <Nusb as libadb::UsbBackend<Tokio>>::Transport,
>;

pub(crate) type Session = libadb::Writer<<Wire as libadb::Splittable>::WriteHalf>;

/// The reader has to run for anything from the device to arrive.
pub(crate) type Reader =
    libadb::Reader<<Wire as libadb::Splittable>::ReadHalf, <Wire as libadb::Splittable>::WriteHalf>;

pub(crate) struct Key {
    /// Each connection parses its own copy, since a handshake holds the key
    /// for as long as the device's owner takes to allow it.
    pem: String,
    name: String,
    tls: TlsClientConfig,
}

impl Key {
    pub(crate) fn load_or_generate(folder: &Path, name: &str) -> Result<Arc<Self>> {
        std::fs::create_dir_all(folder)?;

        let key = store::load_or_generate(folder, &mut OsRng, KeyName::new(name).map_err(failed)?)
            .map_err(failed)?;
        let identity = TlsIdentity::from_key(&key, &mut OsRng).map_err(failed)?;
        let tls = TlsClientConfig::adb(&identity).map_err(failed)?;
        let pem = std::fs::read_to_string(folder.join("adbkey")).context("adbkey")?;

        Ok(Arc::new(Key {
            pem,
            name: name.to_owned(),
            tls,
        }))
    }

    pub(crate) async fn pair(&self, address: &str, code: &str) -> Result<()> {
        let uri = format!("tcp://{address}");

        let Ok(libadb::uri::Uri::Tcp { host, port }) = libadb::uri::parse(&uri) else {
            bail!("invalid address: {address}");
        };

        let name = KeyName::new(&self.name).map_err(failed)?;
        let key = AdbKey::from_pkcs8_pem(&self.pem, &mut OsRng, name).map_err(failed)?;
        let socket = <Tokio as Runtime>::connect_tcp(host, port)
            .await
            .map_err(failed)?;
        let mut transport = libadb::transport::tls::MaybeTls::plain(socket);

        libadb::pairing::pair(&mut transport, &self.tls, &key, code, &mut OsRng)
            .await
            .map_err(failed)?;

        Ok(())
    }
}

struct Asking<'a, F: Fn()> {
    key: &'a mut AdbKey,
    asking: &'a F,
}

impl<F: Fn()> libadb::auth::Authenticator for Asking<'_, F> {
    type Error = <AdbKey as libadb::auth::Authenticator>::Error;

    fn sign(&mut self, token: &[u8]) -> impl Future<Output = Result<Vec<u8>, Self::Error>> {
        self.key.sign(token)
    }

    fn public_key(&self) -> &[u8] {
        self.key.public_key()
    }

    fn offer_public_key(&self) -> bool {
        (self.asking)();

        true
    }
}

pub(crate) async fn connect(
    uri: &str,
    key: &Key,
    asking: impl Fn(),
) -> Result<(Device<Session>, Reader)> {
    let transport = any::connect::<Tokio, Nusb>(uri).await.map_err(failed)?;
    let name = KeyName::new(&key.name).map_err(failed)?;
    let mut own = AdbKey::from_pkcs8_pem(&key.pem, &mut OsRng, name).map_err(failed)?;
    let connection = Connection::connect_tls(
        transport.tls_ready(),
        Asking {
            key: &mut own,
            asking: &asking,
        },
        HostBanner::default(),
        // A screen, a shell, logs and polls each hold channels at once.
        ConnectionConfig::new().with_max_channels(256),
        &key.tls,
    )
    .await
    .map_err(failed)?;
    let (reader, session) = connection.split().map_err(failed)?;

    Ok((Device::new(session), reader))
}

pub(crate) fn on_cables() -> Vec<String> {
    let Ok(found) = nusb::list_devices() else {
        return Vec::new();
    };

    found
        .filter(|device| {
            device.interfaces().any(|interface| {
                (
                    interface.class(),
                    interface.subclass(),
                    interface.protocol(),
                ) == (
                    libadb::ADB_CLASS,
                    libadb::ADB_SUBCLASS,
                    libadb::ADB_PROTOCOL,
                )
            })
        })
        .filter_map(|device| device.serial_number().map(str::to_owned))
        .collect()
}

/// The channel closes once the returned stream is dropped, or when the device
/// closes it.
pub(crate) fn stream(channel: Channel<Session>) -> DuplexStream {
    let (near, far) = tokio::io::duplex(256 * 1024);
    let (mut far_reading, mut far_writing) = tokio::io::split(far);
    let mut reading = channel.handle();
    let mut writing = channel;

    // Each direction runs on its own and is never raced, because dropping a
    // channel read while it acknowledges breaks the whole connection. The
    // device's direction ends when the channel closes, which closing it here
    // also does.
    let from_device = async move {
        let mut bytes = vec![0; 64 * 1024];

        while let Ok(got @ 1..) = reading.read(&mut bytes).await {
            if far_writing.write_all(&bytes[..got]).await.is_err() {
                break;
            }
        }

        // Lets the stream's reader see its end while the half that writes to
        // the device is still held.
        let _ = far_writing.shutdown().await;
    };
    let to_device = async move {
        let mut bytes = vec![0; 64 * 1024];

        while let Ok(sent @ 1..) = far_reading.read(&mut bytes).await {
            if writing.write(&bytes[..sent]).await.is_err() {
                break;
            }
        }

        let _ = writing.close().await;
    };

    tokio::spawn(async move {
        tokio::join!(from_device, to_device);
    });

    near
}

pub(crate) async fn console(port: u16, said: &str) -> Result<String> {
    let token = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".emulator_console_auth_token"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let stream = TcpStream::connect(("127.0.0.1", port)).await?;
    let (reading, mut writing) = stream.into_split();
    let mut lines = BufReader::new(reading).lines();

    console_answer(&mut lines).await?;

    if !token.trim().is_empty() {
        writing
            .write_all(format!("auth {}\n", token.trim()).as_bytes())
            .await?;
        console_answer(&mut lines).await?;
    }

    writing.write_all(format!("{said}\n").as_bytes()).await?;
    let said = console_answer(&mut lines).await?;
    let _ = writing.write_all(b"quit\n").await;

    Ok(said)
}

/// A console answer ends with a line `OK`, or with a line starting with `KO`
/// when the command failed.
async fn console_answer(
    lines: &mut tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
) -> Result<String> {
    let mut said = String::new();

    while let Some(line) = lines.next_line().await? {
        if line.trim_end() == "OK" {
            return Ok(said);
        }

        if let Some(why) = line.strip_prefix("KO") {
            bail!("emulator failed: {}", why.trim_start_matches(':').trim());
        }

        said.push_str(&line);
        said.push('\n');
    }

    bail!("emulator failed: console closed")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Offer {
    Pairing,
    Connecting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Nearby {
    /// Its service name, `adb-<serial>-<random>`, or for a phone pairing by
    /// a QR code the name the code gave.
    pub name: String,
    pub offer: Offer,
    pub address: String,
}

pub(crate) struct Listening {
    daemon: mdns_sd::ServiceDaemon,
    seen: Arc<std::sync::Mutex<Vec<Nearby>>>,
}

impl Listening {
    pub(crate) fn start() -> Result<Self> {
        let daemon = mdns_sd::ServiceDaemon::new().map_err(failed)?;
        let seen: Arc<std::sync::Mutex<Vec<Nearby>>> = Arc::default();

        for (kind, offer) in [
            ("_adb-tls-pairing._tcp.local.", Offer::Pairing),
            ("_adb-tls-connect._tcp.local.", Offer::Connecting),
        ] {
            let events = daemon.browse(kind).map_err(failed)?;
            let seen = seen.clone();
            let suffix = format!(".{kind}");

            std::thread::spawn(move || {
                while let Ok(event) = events.recv() {
                    heard(&mut seen.lock().unwrap(), event, offer, &suffix);
                }
            });
        }

        Ok(Listening { daemon, seen })
    }

    pub(crate) fn nearby(&self) -> Vec<Nearby> {
        self.seen.lock().unwrap().clone()
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}

fn heard(seen: &mut Vec<Nearby>, event: mdns_sd::ServiceEvent, offer: Offer, suffix: &str) {
    match event {
        mdns_sd::ServiceEvent::ServiceResolved(service) => {
            let name = service.get_fullname().trim_end_matches(suffix).to_owned();

            let Some(ip) = service.get_addresses_v4().into_iter().next() else {
                return;
            };

            let address = format!("{ip}:{}", service.get_port());

            seen.retain(|nearby| (nearby.name.as_str(), nearby.offer) != (name.as_str(), offer));
            seen.push(Nearby {
                name,
                offer,
                address,
            });
        }
        mdns_sd::ServiceEvent::ServiceRemoved(_, full) => {
            let name = full.trim_end_matches(suffix);

            seen.retain(|nearby| (nearby.name.as_str(), nearby.offer) != (name, offer));
        }
        _ => {}
    }
}
