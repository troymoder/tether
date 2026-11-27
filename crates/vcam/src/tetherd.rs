use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rustix::io::Errno;
use rustix::net::sockopt::Timeout;
use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType};
use vcam_wire::{ToShim, ToTetherd};

use crate::say::{say_to, start_saying};

pub(crate) const RETRY: Duration = Duration::from_secs(1);

/// The time tetherd has to answer a join.
const JOINING: Duration = Duration::from_secs(2);

/// tetherd's socket path and this emulator's name, from `VSHIM_SOCKET` and
/// `VSHIM_EMULATOR`.
pub(crate) fn tetherd() -> &'static (String, String) {
    static TETHERD: OnceLock<(String, String)> = OnceLock::new();

    TETHERD.get_or_init(|| {
        let variable = |name| std::env::var(name).unwrap_or_default();

        (variable("VSHIM_SOCKET"), variable("VSHIM_EMULATOR"))
    })
}

/// tetherd must run as this user.
pub(crate) fn connect(first: &ToTetherd, wait: Duration) -> io::Result<(OwnedFd, ToShim)> {
    let (socket, _) = tetherd();
    let address = SocketAddrUnix::new(socket.as_str())?;
    let connection = rustix::net::socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )?;

    // A hung tetherd then makes a send fail, which counts as losing it, so
    // the emulator does not hang with it.
    rustix::net::sockopt::set_socket_timeout(&connection, Timeout::Send, Some(wait))?;
    rustix::net::connect(&connection, &address)?;

    let peer = rustix::net::sockopt::socket_peercred(&connection)?;

    if peer.uid != rustix::process::getuid() {
        return Err(io::Error::from(Errno::ACCESS));
    }

    rustix::net::sockopt::set_socket_timeout(&connection, Timeout::Recv, Some(wait))?;
    send(connection.as_fd(), first)?;

    let answer = receive(connection.as_fd(), RecvFlags::empty())?
        .ok_or_else(|| io::Error::from(Errno::CONNRESET))?;

    // After the answer, reads wait as long as the caller does.
    rustix::net::sockopt::set_socket_timeout(&connection, Timeout::Recv, None)?;

    Ok((connection, answer))
}

pub(crate) fn send(to: BorrowedFd, message: &ToTetherd) -> io::Result<()> {
    rustix::net::send(to, &message.encode(), SendFlags::NOSIGNAL)?;

    Ok(())
}

/// With `DONTWAIT`, fails with `EAGAIN` when no message is waiting.
pub(crate) fn receive(from: BorrowedFd, flags: RecvFlags) -> io::Result<Option<ToShim>> {
    let mut packet = vec![0; vcam_wire::MOST];
    let (got, _) = rustix::net::recv(from, &mut packet, flags)?;

    if got == 0 {
        return Ok(None);
    }

    ToShim::decode(&packet[..got])
        .map(Some)
        .ok_or_else(|| io::Error::from(Errno::PROTO))
}

/// The shim joins the first time the devices are needed, then retries at
/// most once per [`RETRY`] until tetherd answers.
pub(crate) fn devices() -> &'static [vcam_wire::Device] {
    static DEVICES: OnceLock<Vec<vcam_wire::Device>> = OnceLock::new();
    static TRIED: Mutex<Option<Instant>> = Mutex::new(None);

    if let Some(devices) = DEVICES.get() {
        return devices;
    }

    let mut tried = TRIED.lock().unwrap();

    // Another thread may have joined meanwhile.
    if let Some(devices) = DEVICES.get() {
        return devices;
    }

    if tried.is_some_and(|tried| tried.elapsed() < RETRY) {
        return &[];
    }

    let first = tried.replace(Instant::now()).is_none();

    if first {
        start_saying();
    }

    match join() {
        Ok(devices) => {
            for device in &devices {
                tracing::info!("{} is {}", device.path, device.format);
            }

            DEVICES.get_or_init(|| devices)
        }
        Err(why) if first => {
            tracing::warn!("no cameras: {why}");

            &[]
        }
        Err(why) => {
            tracing::debug!("no cameras: {why}");

            &[]
        }
    }
}

fn join() -> Result<Vec<vcam_wire::Device>, String> {
    let (socket, emulator) = tetherd();

    if socket.is_empty() || emulator.is_empty() {
        return Err("VSHIM_SOCKET and VSHIM_EMULATOR are not set".to_owned());
    }

    let joining = ToTetherd::Join {
        version: vcam_wire::VERSION,
        emulator: emulator.clone(),
        pid: std::process::id(),
    };

    match connect(&joining, JOINING) {
        Ok((connection, ToShim::Config { devices })) => {
            say_to(connection);

            Ok(devices
                .into_iter()
                .filter(|device| {
                    let good = device.path.starts_with("/dev/") && device.format.is_valid();

                    if !good {
                        tracing::warn!("left alone: {} as {}", device.path, device.format);
                    }

                    good
                })
                .collect())
        }
        Ok((_, ToShim::Err(why))) => Err(format!("turned down: {why}")),
        Ok(_) => Err("tetherd did not answer".to_owned()),
        Err(error) => Err(format!("no tetherd: {error}")),
    }
}
