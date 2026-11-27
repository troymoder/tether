//! tetherd's one TCP port carries HTTP and ICE over TCP, told apart by their
//! first bytes (`is_http`). The same port number on UDP, on each address of
//! this machine, carries ICE over UDP. One task (drive.rs) drives every
//! connection's `Rtc` (str0m), which does no I/O of its own.
//!
//! Only a browser that tetherd trusts gets a connection (`Hub::accept`, after
//! the key handshake of `protocol::CONNECT`). Packets from anyone else match no
//! `Rtc` and are dropped.

mod drive;
mod stun;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use str0m::change::SdpOffer;
use str0m::channel::ChannelId;
use str0m::media::Mid;
use str0m::net::{Protocol, TcpType};
use str0m::{Candidate, Rtc, RtcConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::{Semaphore, mpsc, oneshot};

use stun::{Outside, STUN_EVERY};

pub(crate) type Peer = u64;

/// The receiver of the task that serves the browser ends when the connection
/// does.
pub(crate) enum Said {
    Opened(ChannelId, String),
    Data(ChannelId, Vec<u8>),
    Closed(ChannelId),
    KeyFrame(Mid),
}

/// A frame of H.264 as Annex B (`profile` its `profile_idc`), or a packet of
/// Opus, by the time it shows in microseconds.
pub(crate) struct Sample {
    pub(crate) track: Track,
    pub(crate) time: u64,
    pub(crate) key: bool,
    pub(crate) data: Arc<[u8]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Track {
    Video { profile: u8 },
    Audio,
}

/// Bytes that hold a place in a sender's window until the connection has
/// taken them. This keeps a fast device from burying a slow browser.
pub(crate) struct Credit(pub(crate) Arc<Semaphore>, pub(crate) u32);

enum Asked {
    Join {
        peer: Peer,
        rtc: Box<Rtc>,
        said: mpsc::UnboundedSender<Said>,
    },

    Receive {
        proto: Protocol,
        from: SocketAddr,
        to: SocketAddr,
        data: Vec<u8>,
    },

    TcpOpened(SocketAddr, mpsc::UnboundedSender<Vec<u8>>),
    TcpClosed(SocketAddr),
    Write {
        peer: Peer,
        channel: ChannelId,
        data: Vec<u8>,
        credit: Option<Credit>,
    },

    Close(Peer, ChannelId),
    Leave(Peer),
    Offer {
        peer: Peer,
        offer: SdpOffer,
        answer: oneshot::Sender<Result<String, String>>,
    },
    Media {
        peer: Peer,
        mid: Mid,
        sample: Arc<Sample>,
    },
}

type Sockets = Arc<Mutex<HashMap<IpAddr, Arc<UdpSocket>>>>;

#[derive(Clone)]
pub(crate) struct Hub {
    asked: mpsc::UnboundedSender<Asked>,
    port: u16,
    sockets: Sockets,
    next: Arc<AtomicU64>,
    stun: Option<Arc<str>>,
    outside: Arc<Outside>,
}

/// True if a connection to the port is HTTP. A request starts with its method
/// in capitals (`GET `, `POST`). ICE over TCP starts with the length of a
/// STUN message (RFC 4571), whose first byte is 0.
pub(crate) async fn is_http(stream: &TcpStream) -> std::io::Result<bool> {
    let mut first = [0u8; 3];

    loop {
        let seen = stream.peek(&mut first).await?;

        if seen == 0 || first[..seen].iter().any(|byte| !byte.is_ascii_uppercase()) {
            return Ok(false);
        }

        if seen == first.len() {
            return Ok(true);
        }
    }
}

impl Hub {
    pub(crate) fn start(port: u16, stun: Option<String>) -> Self {
        let (asked, waiting) = mpsc::unbounded_channel();
        let sockets = Sockets::default();

        tokio::spawn(drive::run(waiting, sockets.clone()));

        let hub = Self {
            asked,
            port,
            sockets,
            next: Arc::default(),
            stun: stun.map(Arc::from),
            outside: Arc::default(),
        };

        if hub.stun.is_some() {
            tokio::spawn(hub.clone().look_outside());
        }

        hub
    }

    pub(crate) fn stun(&self) -> Option<&str> {
        self.stun.as_deref()
    }

    async fn look_outside(self) {
        let Some(server) = self.stun.clone() else {
            return;
        };

        loop {
            match tokio::net::lookup_host(&*server).await {
                Ok(found) => self.ask_stun(&found.collect::<Vec<_>>()),
                Err(error) => tracing::warn!("stun failed: {server}: {error}"),
            }

            tokio::time::sleep(STUN_EVERY).await;
            // Requests still unanswered by now are taken as lost.
            self.outside.asked.lock().unwrap().clear();
        }
    }

    fn ask_stun(&self, server: &[SocketAddr]) {
        for socket in self.udp_sockets() {
            let Ok(from) = socket.local_addr() else {
                continue;
            };

            // Only to an address of the same family, and never from loopback,
            // which nothing outside sees.
            let Some(to) = server
                .iter()
                .find(|to| to.is_ipv4() == from.is_ipv4() && !from.ip().is_loopback())
            else {
                continue;
            };

            let id = crate::trust::random::<12>();

            self.outside.asked.lock().unwrap().insert(id, from);
            let _ = socket.try_send_to(&stun::request(id), *to);
        }
    }

    pub(crate) fn accept(
        &self,
        offer: &str,
    ) -> Result<(Peer, String, mpsc::UnboundedReceiver<Said>)> {
        let offer = SdpOffer::from_sdp_string(offer).context("webrtc failed: invalid offer")?;
        let mut config = RtcConfig::new();

        // str0m checks a working connection every 3 s and gives up on its
        // path after 9 retransmits, about 21 s. Checked every 500 ms with 4
        // retransmits it is about 2 s (500 × 3 + 500 ms, str0m's
        // StunTiming::timeout), and the connection is then disconnected but
        // may recover for 30 s (drive.rs `RECOVERING`). The checks also keep
        // the browser hearing from a live connection.
        config.set_max_stun_rto(Duration::from_millis(500));
        config.set_max_stun_retransmits(4);

        let mut rtc = config.build(Instant::now());

        for socket in self.udp_sockets() {
            rtc.add_local_candidate(Candidate::host(socket.local_addr()?, "udp")?);
        }

        let seen = self.outside.seen.lock().unwrap().clone();

        for (base, (outside, when)) in seen {
            if outside != base && when.elapsed() < 3 * STUN_EVERY {
                rtc.add_local_candidate(Candidate::server_reflexive(outside, base, "udp")?);
            }
        }

        // The TCP port only listens on this machine, which is where a port
        // forwarded to it arrives.
        let tcp = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port);

        rtc.add_local_candidate(
            Candidate::builder()
                .tcp()
                .host(tcp)
                .tcptype(TcpType::Passive)
                .build()?,
        );

        let answer = rtc.sdp_api().accept_offer(offer)?.to_sdp_string();
        let peer = self.next.fetch_add(1, Ordering::Relaxed);
        let (said, hearing) = mpsc::unbounded_channel();
        let rtc = Box::new(rtc);
        let _ = self.asked.send(Asked::Join { peer, rtc, said });

        Ok((peer, answer, hearing))
    }

    /// A credit's window is given back once the connection has taken the
    /// message.
    pub(crate) fn write(
        &self,
        peer: Peer,
        channel: ChannelId,
        data: Vec<u8>,
        credit: Option<Credit>,
    ) {
        let _ = self.asked.send(Asked::Write {
            peer,
            channel,
            data,
            credit,
        });
    }

    /// Ends a channel. Once the messages before it have gone, a text message
    /// asks the page to close it.
    // Only the page resets channels. The browser reuses a stream id as soon
    // as its own resets of it are done, and a reset from tetherd that crossed
    // the page's could leave tetherd's SCTP holding the id a while longer.
    // str0m then fails the open of the next channel on that id and closes it
    // before its first message (rtc/drive.rs `first_message_comes_before_close`).
    pub(crate) fn close(&self, peer: Peer, channel: ChannelId) {
        let _ = self.asked.send(Asked::Close(peer, channel));
    }

    pub(crate) async fn renegotiate(&self, peer: Peer, offer: &str) -> Result<String> {
        let offer = SdpOffer::from_sdp_string(offer).context("webrtc failed: invalid offer")?;
        let (answer, answered) = oneshot::channel();
        let _ = self.asked.send(Asked::Offer {
            peer,
            offer,
            answer,
        });

        answered
            .await
            .context("webrtc failed: the connection ended")?
            .map_err(|error| anyhow::anyhow!("webrtc failed: {error}"))
    }

    pub(crate) fn media(&self, peer: Peer, mid: Mid, sample: Arc<Sample>) {
        let _ = self.asked.send(Asked::Media { peer, mid, sample });
    }

    pub(crate) fn leave(&self, peer: Peer) {
        let _ = self.asked.send(Asked::Leave(peer));
    }

    /// ICE over TCP has each packet after its length in two bytes (RFC 4571).
    pub(crate) async fn tcp(&self, stream: TcpStream, from: SocketAddr) {
        let Ok(to) = stream.local_addr() else {
            return;
        };

        let (mut reading, mut writing) = stream.into_split();
        let (out, mut outgoing) = mpsc::unbounded_channel::<Vec<u8>>();
        let _ = self.asked.send(Asked::TcpOpened(from, out));
        let sending = tokio::spawn(async move {
            while let Some(packet) = outgoing.recv().await {
                let length = (packet.len() as u16).to_be_bytes();

                if writing.write_all(&length).await.is_err()
                    || writing.write_all(&packet).await.is_err()
                {
                    return;
                }
            }
        });

        loop {
            let mut length = [0u8; 2];

            if reading.read_exact(&mut length).await.is_err() {
                break;
            }

            let mut data = vec![0; u16::from_be_bytes(length).into()];

            if reading.read_exact(&mut data).await.is_err() {
                break;
            }

            let receive = Asked::Receive {
                proto: Protocol::Tcp,
                from,
                to,
                data,
            };

            if self.asked.send(receive).is_err() {
                break;
            }
        }

        sending.abort();
        let _ = self.asked.send(Asked::TcpClosed(from));
    }

    /// Binds a socket on each address of this machine that has none yet, so a
    /// VPN that came up since the last call gets one.
    fn udp_sockets(&self) -> Vec<Arc<UdpSocket>> {
        let addresses = if_addrs::get_if_addrs().unwrap_or_default();
        let mut sockets = self.sockets.lock().unwrap();

        for address in addresses {
            let ip = address.ip();
            // Link-local addresses cannot be reached as they are written.
            let usable = match ip {
                IpAddr::V4(ip) => !ip.is_link_local(),
                IpAddr::V6(ip) => !ip.is_unicast_link_local(),
            };

            if !usable || sockets.contains_key(&ip) {
                continue;
            }

            let bound =
                std::net::UdpSocket::bind(SocketAddr::new(ip, self.port)).and_then(|socket| {
                    socket.set_nonblocking(true)?;
                    sized(&socket);
                    UdpSocket::from_std(socket)
                });

            match bound {
                Ok(socket) => {
                    let socket = Arc::new(socket);

                    tokio::spawn(receive(
                        socket.clone(),
                        self.asked.clone(),
                        self.outside.clone(),
                    ));

                    sockets.insert(ip, socket);
                }
                Err(error) => tracing::warn!("webrtc failed: udp on {ip}: {error}"),
            }
        }

        sockets.values().cloned().collect()
    }
}

/// The send and receive buffers asked of each UDP socket. The default, about
/// 200 KB on Linux, overflows on an iPhone's key frames at 20 Mbit/s, and a
/// packet that does not fit is lost (drive.rs). The data channel then waits
/// for SCTP to resend it, which freezes the screen for half a second or more.
/// Linux caps the size at `net.core.wmem_max` and `rmem_max`, which were 4 MB
/// on the machines this was tuned on.
const SOCKET_BUFFER: usize = 4 * 1024 * 1024;

fn sized(socket: &std::net::UdpSocket) {
    if let Err(error) = rustix::net::sockopt::set_socket_send_buffer_size(socket, SOCKET_BUFFER) {
        tracing::debug!("webrtc: send buffer: {error}");
    }

    if let Err(error) = rustix::net::sockopt::set_socket_recv_buffer_size(socket, SOCKET_BUFFER) {
        tracing::debug!("webrtc: receive buffer: {error}");
    }
}

async fn receive(
    socket: Arc<UdpSocket>,
    asked: mpsc::UnboundedSender<Asked>,
    outside: Arc<Outside>,
) {
    let Ok(to) = socket.local_addr() else {
        return;
    };

    let mut buffer = vec![0; 2048];

    while let Ok((got, from)) = socket.recv_from(&mut buffer).await {
        // The STUN server's answers are for `Outside`, and match no connection.
        if outside.heard(&buffer[..got]) {
            continue;
        }

        let receive = Asked::Receive {
            proto: Protocol::Udp,
            from,
            to,
            data: buffer[..got].to_vec(),
        };

        if asked.send(receive).is_err() {
            return;
        }
    }
}
