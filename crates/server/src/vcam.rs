//! The shim in each emulator (crates/vcam) gets its cameras and frames from one
//! `SOCK_SEQPACKET` Unix socket in `$XDG_RUNTIME_DIR/tetherd-<port>`, and
//! vcam-wire describes what is said on it.
//!
//! An emulator is started with this socket and its own name. The shim joins
//! with the name and is given the emulator's cameras, then connects once for
//! each stream (stream.rs). Only a process of this user that is the emulator
//! tetherd started under that name, or a descendant of it, is let in.

mod stream;

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use tokio::sync::watch;
use tokio_seqpacket::ancillary::OwnedAncillaryMessage;
use tokio_seqpacket::{UnixSeqpacket, UnixSeqpacketListener};
pub(crate) use vcam_wire::{Format, FourCc, Range};
use vcam_wire::{ToShim, ToTetherd};

use self::stream::stream_frames;

pub(crate) struct Lens {
    /// The device the shim stands in for, `/dev/video0` and on.
    pub device: String,
    pub format: Format,
    pub frames: watch::Receiver<Arc<Vec<u8>>>,
}

pub(crate) type Log = Arc<dyn Fn(&str) + Send + Sync>;

struct Emulator {
    pid: u32,
    lenses: Arc<Vec<Lens>>,
    log: Log,
}

pub(crate) struct Cameras {
    socket: PathBuf,
    emulators: Mutex<HashMap<String, Emulator>>,
    sessions: AtomicU32,
}

impl Cameras {
    pub(crate) fn listen(port: u16) -> Result<Arc<Self>> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let folder = runtime.join(format!("tetherd-{port}"));

        std::fs::create_dir_all(&folder).context(folder.display().to_string())?;
        set_mode(&folder, 0o700)?;

        let socket = folder.join("control.sock");
        // A tetherd that crashed may have left one behind.
        let _ = std::fs::remove_file(&socket);
        let listener =
            UnixSeqpacketListener::bind(&socket).context(socket.display().to_string())?;

        set_mode(&socket, 0o600)?;

        let cameras = Arc::new(Cameras {
            socket,
            emulators: Mutex::default(),
            sessions: AtomicU32::new(0),
        });

        tokio::spawn(cameras.clone().serve(listener));

        Ok(cameras)
    }

    async fn serve(self: Arc<Self>, mut listener: UnixSeqpacketListener) {
        loop {
            let connection = match listener.accept().await {
                Ok(connection) => connection,
                // Errors such as running out of fds pass, so it tries again.
                Err(error) => {
                    tracing::warn!("camera socket: {error}");
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    continue;
                }
            };
            let cameras = self.clone();

            tokio::spawn(async move {
                if let Err(error) = cameras.session(connection).await {
                    tracing::warn!("camera shim: {error:#}");
                }
            });
        }
    }

    pub(crate) fn socket(&self) -> &Path {
        &self.socket
    }

    pub(crate) fn add(&self, name: &str, pid: u32, lenses: Vec<Lens>, log: Log) {
        let lenses = Arc::new(lenses);

        self.emulators
            .lock()
            .unwrap()
            .insert(name.to_owned(), Emulator { pid, lenses, log });
    }

    pub(crate) fn remove(&self, name: &str) {
        self.emulators.lock().unwrap().remove(name);
    }

    fn of(&self, name: &str, pid: Option<i32>) -> Result<(Arc<Vec<Lens>>, Log)> {
        let emulators = self.emulators.lock().unwrap();
        let emulator = emulators
            .get(name)
            .with_context(|| format!("no emulator {name}"))?;
        let pid = pid.and_then(|pid| u32::try_from(pid).ok());

        ensure!(
            pid.is_some_and(|pid| descends(pid, emulator.pid)),
            "not a process of {name}"
        );

        Ok((emulator.lenses.clone(), emulator.log.clone()))
    }

    async fn session(&self, connection: UnixSeqpacket) -> Result<()> {
        let peer = connection.peer_cred()?;

        // The socket's folder is this user's only too.
        ensure!(
            peer.uid() == rustix::process::getuid().as_raw(),
            "another user"
        );

        let Some((first, _)) = receive(&connection).await? else {
            return Ok(());
        };

        match first {
            ToTetherd::Join {
                version,
                emulator,
                pid: _,
            } => self.join(&connection, version, &emulator, peer.pid()).await,
            ToTetherd::Hello {
                version,
                emulator,
                pid: _,
                device,
                format,
            } => {
                self.hello(&connection, version, &emulator, peer.pid(), &device, format)
                    .await
            }
            other => bail!("not a start: {other:?}"),
        }
    }

    async fn join(
        &self,
        connection: &UnixSeqpacket,
        version: u32,
        emulator: &str,
        pid: Option<i32>,
    ) -> Result<()> {
        let (lenses, log) = match check(version).and_then(|()| self.of(emulator, pid)) {
            Ok(found) => found,
            Err(error) => return refuse(connection, error).await,
        };
        let devices = lenses
            .iter()
            .map(|lens| vcam_wire::Device {
                path: lens.device.clone(),
                format: lens.format,
            })
            .collect();

        send(connection, &ToShim::Config { devices }).await?;

        while let Some((message, _)) = receive(connection).await? {
            if let ToTetherd::Log { level, message } = message {
                let line = format!("camera {level:?}: {message}");

                tracing::info!(%emulator, "{line}");
                log(&line);
            }
        }

        Ok(())
    }

    async fn hello(
        &self,
        connection: &UnixSeqpacket,
        version: u32,
        emulator: &str,
        pid: Option<i32>,
        device: &str,
        format: Format,
    ) -> Result<()> {
        let lenses = match check(version).and_then(|()| self.of(emulator, pid)) {
            Ok((lenses, _)) => lenses,
            Err(error) => return refuse(connection, error).await,
        };

        let Some(lens) = lenses.iter().find(|lens| lens.device == device) else {
            return refuse(connection, anyhow::anyhow!("no camera {device}")).await;
        };

        if lens.format != format {
            let error = anyhow::anyhow!("{device} is {}, not {format}", lens.format);

            return refuse(connection, error).await;
        }

        // The source is gone for good, so the shim keeps the camera black.
        if lens.frames.has_changed().is_err() {
            return refuse(connection, anyhow::anyhow!("{device} has no source")).await;
        }

        let session = self.sessions.fetch_add(1, Ordering::Relaxed);

        send(connection, &ToShim::HelloOk { session }).await?;
        stream_frames(lens, connection).await
    }
}

fn check(version: u32) -> Result<()> {
    ensure!(
        version == vcam_wire::VERSION,
        "version {version}, not {}",
        vcam_wire::VERSION
    );

    Ok(())
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .context(path.display().to_string())
}

fn descends(mut pid: u32, ancestor: u32) -> bool {
    // A real process tree is never this deep, so a longer chain is refused.
    for _ in 0..64 {
        if pid == ancestor {
            return true;
        }

        let Some(parent) = parent(pid) else {
            return false;
        };

        if parent == 0 || parent == pid {
            return false;
        }

        pid = parent;
    }

    false
}

/// The parent of a process, the fourth field of its `/proc` stat. It is read
/// after the name, which is in brackets and may contain anything.
fn parent(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, after) = stat.rsplit_once(')')?;

    after.split_whitespace().nth(1)?.parse().ok()
}

async fn refuse(connection: &UnixSeqpacket, error: anyhow::Error) -> Result<()> {
    send(connection, &ToShim::Err(format!("{error:#}"))).await?;

    Err(error)
}

async fn send(connection: &UnixSeqpacket, message: &ToShim) -> Result<()> {
    connection.send(&message.encode()).await?;

    Ok(())
}

async fn receive(connection: &UnixSeqpacket) -> Result<Option<(ToTetherd, Option<OwnedFd>)>> {
    let mut packet = vec![0; vcam_wire::MOST];
    let mut extra = [0; 64];
    let (got, ancillary) = connection
        .recv_with_ancillary(&mut packet, &mut extra)
        .await?;
    let fd = ancillary.into_messages().find_map(|message| match message {
        OwnedAncillaryMessage::FileDescriptors(mut fds) => fds.next(),
        _ => None,
    });

    if got.bytes_read() == 0 {
        return Ok(None);
    }

    let message = ToTetherd::decode(&packet[..got.bytes_read()]).context("not a message")?;

    Ok(Some((message, fd)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_its_own_process_and_its_parents() {
        let me = std::process::id();
        let parent = parent(me).unwrap();

        assert!(descends(me, me));
        assert!(descends(me, parent));
        assert!(!descends(parent, me));
    }
}
