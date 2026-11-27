//! A byte stream over a pair of channels, so that idevice's services can use
//! a service's stream that is carried somewhere else, such as the page's
//! WebRTC channel to tetherd (crates/web/src/link/ios.rs).

use std::io;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

use futures_channel::mpsc;
use futures_util::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct Pipe {
    // Behind a mutex only so the stream is `Sync`, which idevice asks.
    incoming: Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    held: Vec<u8>,
    outgoing: mpsc::UnboundedSender<Vec<u8>>,
}

/// Dropping `incoming` ends the stream for its reader.
pub struct Far {
    pub incoming: mpsc::UnboundedSender<Vec<u8>>,
    pub outgoing: mpsc::UnboundedReceiver<Vec<u8>>,
}

pub fn pipe() -> (Pipe, Far) {
    let (incoming, read) = mpsc::unbounded();
    let (written, outgoing) = mpsc::unbounded();
    let pipe = Pipe {
        incoming: Mutex::new(read),
        held: Vec::new(),
        outgoing: written,
    };

    (pipe, Far { incoming, outgoing })
}

impl std::fmt::Debug for Pipe {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Pipe")
    }
}

impl AsyncRead for Pipe {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        into: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.held.is_empty() {
            let next = {
                let mut incoming = self
                    .incoming
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);

                incoming.poll_next_unpin(context)
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

impl AsyncWrite for Pipe {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.outgoing.unbounded_send(data.to_vec()) {
            Ok(()) => Poll::Ready(Ok(data.len())),
            Err(_) => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.outgoing.close_channel();

        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn bytes_pass_both_ways_and_the_far_end_closes_it() {
        let (mut pipe, mut far) = pipe();

        pipe.write_all(b"out").await.unwrap();
        assert_eq!(far.outgoing.next().await.unwrap(), b"out");

        far.incoming.unbounded_send(b"in".to_vec()).unwrap();
        drop(far.incoming);

        let mut read = Vec::new();

        pipe.read_to_end(&mut read).await.unwrap();
        assert_eq!(read, b"in");
    }
}
