//! The mux knows nothing of USB. A transport feeds it what the device sends
//! ([`Mux::receive`]) and sends the device what it queues, so any USB stack
//! can drive it.
//!
//! Each connection acknowledges what it receives at once and advertises a
//! fixed window, as webmuxd does. What the device sends is queued for the
//! reader without bound. A writer is held back once a window's worth waits
//! to go out.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};

use anyhow::{Result, anyhow, bail};
use futures_channel::{mpsc, oneshot};
use futures_util::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use self::wire::{ACK, FIN, Gather, RST, SYN, Segment};

mod wire;

const WINDOW: usize = 128 * 1024;
const MOST_DATA: usize = wire::MOST_OUT - 16 - wire::TCP_HEADER;

#[derive(Clone)]
pub struct Mux(Arc<Mutex<Inner>>);

impl std::fmt::Debug for Mux {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Mux")
    }
}

struct Inner {
    /// 0 until the device answers.
    version: u32,
    sent: u16,
    heard: u16,
    out: mpsc::UnboundedSender<Vec<u8>>,
    gather: Gather,
    ready: Option<oneshot::Sender<Result<(), String>>>,
    connections: HashMap<u16, Connection>,
    next_port: u16,
    closed: bool,
}

struct Connection {
    to: u16,
    opening: Option<oneshot::Sender<Result<(), String>>>,
    sent: u32,
    expected: u32,
    acknowledged: u32,
    /// The device's window, in bytes.
    window: usize,
    waiting: VecDeque<Vec<u8>>,
    /// The writer held back while `waiting` is full.
    writer: Option<Waker>,
    incoming: mpsc::UnboundedSender<Vec<u8>>,
}

impl Connection {
    fn queued(&self) -> usize {
        self.waiting.iter().map(Vec::len).sum()
    }
}

/// Wakes a held-back writer, so it sees the connection is gone.
impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(writer) = self.writer.take() {
            writer.wake();
        }
    }
}

impl Mux {
    /// The receiver gives the packets the transport is to send the device, in
    /// order.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (out, packets) = mpsc::unbounded();
        let inner = Inner {
            version: 0,
            sent: 0,
            heard: 0xffff,
            out,
            gather: Gather::default(),
            ready: None,
            connections: HashMap::new(),
            next_port: 1,
            closed: false,
        };

        (Self(Arc::new(Mutex::new(inner))), packets)
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Connections can be made once this returns.
    pub async fn start(&self) -> Result<()> {
        let (told, answer) = oneshot::channel();

        {
            let mut inner = self.inner();

            inner.ready = Some(told);
            inner.send(wire::VERSION, &wire::version(2));
        }

        answer
            .await
            .map_err(|_| anyhow!("usbmux failed: device gone"))?
            .map_err(|said| anyhow!(said))
    }

    pub fn receive(&self, chunk: &[u8]) {
        let mut inner = self.inner();
        let version = inner.version;
        let packets = match inner.gather.push(chunk, version) {
            Ok(packets) => packets,
            Err(said) => {
                tracing::warn!("{said}");

                return;
            }
        };

        for packet in packets {
            inner.handle(packet);
        }
    }

    pub fn close(&self) {
        let mut inner = self.inner();

        inner.closed = true;
        inner.connections.clear();

        if let Some(ready) = inner.ready.take() {
            let _ = ready.send(Err("usbmux failed: device gone".to_owned()));
        }
    }

    pub async fn connect(&self, port: u16) -> Result<Stream> {
        let (told, answer) = oneshot::channel();
        let (incoming, received) = mpsc::unbounded();
        let from = {
            let mut inner = self.inner();

            if inner.closed {
                bail!("usbmux failed: device gone");
            }

            let from = inner.free_port()?;

            inner.connections.insert(
                from,
                Connection {
                    to: port,
                    opening: Some(told),
                    sent: 0,
                    expected: 0,
                    acknowledged: 0,
                    window: 0,
                    waiting: VecDeque::new(),
                    writer: None,
                    incoming,
                },
            );
            inner.send_segment(from, SYN, &[]);

            from
        };

        let opened = answer
            .await
            .map_err(|_| anyhow!("usbmux failed: device gone"))?;

        if let Err(said) = opened {
            self.inner().connections.remove(&from);
            bail!(said);
        }

        Ok(Stream {
            mux: self.clone(),
            port: from,
            received: Mutex::new(received),
            held: Vec::new(),
        })
    }
}

impl Inner {
    fn send(&mut self, protocol: u32, payload: &[u8]) {
        if protocol == wire::SETUP {
            self.sent = 0;
            self.heard = 0xffff;
        }

        let packet = wire::packet(protocol, payload, self.version, (self.sent, self.heard));

        if protocol != wire::VERSION && self.version >= 2 {
            self.sent = self.sent.wrapping_add(1);
        }

        let _ = self.out.unbounded_send(packet);
    }

    fn send_segment(&mut self, from: u16, flags: u8, data: &[u8]) {
        let Some(connection) = self.connections.get(&from) else {
            return;
        };
        let header = Segment {
            from,
            to: connection.to,
            sequence: connection.sent,
            acknowledged: connection.expected,
            flags,
            window: (WINDOW >> 8) as u16,
        };

        self.send(wire::TCP, &wire::segment(header, data));
    }

    fn free_port(&mut self) -> Result<u16> {
        for _ in 0..u16::MAX {
            let port = self.next_port;

            self.next_port = self.next_port.checked_add(1).unwrap_or(1);

            if !self.connections.contains_key(&port) {
                return Ok(port);
            }
        }

        bail!("usbmux failed: no free port")
    }

    fn handle(&mut self, packet: wire::Packet) {
        if packet.protocol != wire::VERSION && self.version >= 2 {
            match packet.magic {
                Some((wire::RECEIVED, heard)) => self.heard = heard,
                _ => return,
            }
        }

        match packet.protocol {
            wire::VERSION => self.versioned(&packet.payload),
            wire::CONTROL => {
                let said = String::from_utf8_lossy(packet.payload.get(1..).unwrap_or_default());

                tracing::debug!("usbmux: {}", said.trim_end_matches('\0'));
            }
            wire::TCP => {
                if let Some((header, data)) = wire::parse_segment(&packet.payload) {
                    self.segment(header, data);
                }
            }
            _ => {}
        }
    }

    fn versioned(&mut self, payload: &[u8]) {
        let Some(ready) = self.ready.take() else {
            return;
        };

        match wire::major(payload) {
            Some(major @ (1 | 2)) => {
                self.version = major;

                if major == 2 {
                    self.send(wire::SETUP, &[7]);
                }

                let _ = ready.send(Ok(()));
            }
            other => {
                let _ = ready.send(Err(format!("usbmux failed: version {other:?}")));
            }
        }
    }

    fn segment(&mut self, header: Segment, data: &[u8]) {
        let from = header.to;
        let Some(connection) = self.connections.get_mut(&from) else {
            return;
        };

        if connection.to != header.from {
            return;
        }

        connection.acknowledged = header.acknowledged;
        connection.window = usize::from(header.window) << 8;

        if header.flags & RST != 0 {
            if let Some(opening) = self
                .connections
                .remove(&from)
                .and_then(|mut gone| gone.opening.take())
            {
                let _ = opening.send(Err(format!("usbmux failed: port {} refused", header.from)));
            }

            return;
        }

        if let Some(opening) = connection.opening.take() {
            if header.flags != SYN | ACK {
                let _ = opening.send(Err(format!("usbmux failed: port {} refused", header.from)));
                self.send_segment(from, RST, &[]);
                self.connections.remove(&from);

                return;
            }

            connection.sent = connection.sent.wrapping_add(1);
            connection.expected = header.sequence.wrapping_add(1);

            let _ = opening.send(Ok(()));

            self.send_segment(from, ACK, &[]);
            self.flush(from);

            return;
        }

        if !data.is_empty() {
            if header.sequence == connection.expected {
                connection.expected = connection.expected.wrapping_add(data.len() as u32);

                let _ = connection.incoming.unbounded_send(data.to_vec());
            } else {
                tracing::debug!("usbmux: segment out of order on port {from}");
            }
        }

        let finished = header.flags & FIN != 0;

        if finished {
            connection.expected = connection.expected.wrapping_add(1);
        }

        if !data.is_empty() || finished {
            self.send_segment(from, ACK, &[]);
        }

        if finished {
            self.send_segment(from, RST | ACK, &[]);
            self.connections.remove(&from);

            return;
        }

        self.flush(from);
    }

    fn flush(&mut self, from: u16) {
        loop {
            let Some(connection) = self.connections.get_mut(&from) else {
                return;
            };

            if connection.opening.is_some() {
                return;
            }

            if connection.queued() < WINDOW
                && let Some(writer) = connection.writer.take()
            {
                writer.wake();
            }

            let in_flight = connection.sent.wrapping_sub(connection.acknowledged) as usize;
            let room = connection.window.saturating_sub(in_flight).min(MOST_DATA);
            let Some(next) = connection.waiting.front_mut() else {
                return;
            };

            if room == 0 {
                tracing::debug!("usbmux: port {from} waits on the device's window");

                return;
            }

            let chunk: Vec<u8> = if next.len() <= room {
                connection.waiting.pop_front().unwrap_or_default()
            } else {
                next.drain(..room).collect()
            };

            self.send_segment(from, ACK, &chunk);

            if let Some(connection) = self.connections.get_mut(&from) {
                connection.sent = connection.sent.wrapping_add(chunk.len() as u32);
            }
        }
    }
}

pub struct Stream {
    mux: Mux,
    port: u16,
    // Behind a mutex only so the stream is `Sync`, which idevice asks.
    received: Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    held: Vec<u8>,
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "usbmux connection {}", self.port)
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        into: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.held.is_empty() {
            let next = {
                let mut received = self
                    .received
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);

                received.poll_next_unpin(context)
            };

            match next {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Ready(Some(data)) => self.held = data,
            }
        }

        let taken = self.held.len().min(into.remaining());

        into.put_slice(&self.held[..taken]);
        self.held.drain(..taken);

        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for Stream {
    /// A writer that is never held back queues without end, and a TCP stack
    /// writing through it, as the tunnel to the developer services does,
    /// then times out its retransmissions waiting behind its own backlog.
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut inner = self.mux.inner();
        let Some(connection) = inner.connections.get_mut(&self.port) else {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        };

        if connection.queued() >= WINDOW {
            connection.writer = Some(context.waker().clone());

            return Poll::Pending;
        }

        connection.waiting.push_back(data.to_vec());
        inner.flush(self.port);

        Poll::Ready(Ok(data.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let mut inner = self.mux.inner();

        if inner.connections.contains_key(&self.port) {
            inner.send_segment(self.port, RST | ACK, &[]);
            inner.connections.remove(&self.port);
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    fn from_phone(protocol: u32, payload: &[u8]) -> Vec<u8> {
        let mut packet = wire::packet(protocol, payload, 2, (0, 0));

        packet[8..12].copy_from_slice(&wire::RECEIVED.to_be_bytes());
        packet
    }

    fn sent_segment(packet: &[u8]) -> (Segment, Vec<u8>) {
        let (header, data) = wire::parse_segment(&packet[16..]).unwrap();

        (header, data.to_vec())
    }

    #[tokio::test]
    async fn connects_and_carries_data_both_ways() {
        let (mux, mut out) = Mux::new();
        let phone = async {
            let version = out.next().await.unwrap();

            assert_eq!(&version[..4], [0, 0, 0, 0]);

            mux.receive(&wire::packet(wire::VERSION, &wire::version(2), 0, (0, 0)));

            let setup = out.next().await.unwrap();

            assert_eq!(&setup[..4], [0, 0, 0, 2]);
        };

        let (started, ()) = tokio::join!(mux.start(), phone);

        started.unwrap();

        let phone = async {
            let (syn, _) = sent_segment(&out.next().await.unwrap());

            assert_eq!((syn.flags, syn.to), (SYN, 62078));

            let accept = Segment {
                from: 62078,
                to: syn.from,
                sequence: 100,
                acknowledged: 1,
                flags: SYN | ACK,
                window: 512,
            };

            mux.receive(&from_phone(wire::TCP, &wire::segment(accept, &[])));

            let (ack, _) = sent_segment(&out.next().await.unwrap());

            assert_eq!((ack.flags, ack.acknowledged), (ACK, 101));

            syn.from
        };

        let (stream, port) = tokio::join!(mux.connect(62078), phone);
        let mut stream = stream.unwrap();

        stream.write_all(b"hello").await.unwrap();

        let (sent, data) = sent_segment(&out.next().await.unwrap());

        assert_eq!((sent.sequence, data.as_slice()), (1, &b"hello"[..]));

        let reply = Segment {
            from: 62078,
            to: port,
            sequence: 101,
            acknowledged: 6,
            flags: ACK,
            window: 512,
        };

        mux.receive(&from_phone(wire::TCP, &wire::segment(reply, b"world")));

        let mut read = [0; 5];

        stream.read_exact(&mut read).await.unwrap();
        assert_eq!(&read, b"world");

        let (ack, _) = sent_segment(&out.next().await.unwrap());

        assert_eq!(ack.acknowledged, 106);
    }
}
