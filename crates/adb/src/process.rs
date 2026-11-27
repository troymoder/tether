use anyhow::Result;
use libadb::Session;
use libadb::shell::v2::{FrameId, WindowSize};

use crate::Channel;

#[derive(Clone, Copy)]
pub struct Terminal<'a> {
    pub term: &'a str,
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, PartialEq)]
pub enum Said {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(u8),
}

pub struct Process<S> {
    pub input: Input<S>,
    pub output: Output<S>,
}

impl<S: Clone> Process<S> {
    pub(crate) fn new(channel: Channel<S>) -> Self {
        Process {
            input: Input {
                channel: channel.handle(),
            },
            output: Output {
                channel,
                pending: Vec::new(),
                ended: false,
            },
        }
    }
}

pub struct Input<S> {
    channel: Channel<S>,
}

pub struct Output<S> {
    channel: Channel<S>,
    pending: Vec<u8>,
    ended: bool,
}

/// The largest input frame sent, well under what a device accepts.
const MOST_INPUT: usize = 32 * 1024;

impl<S> Input<S>
where
    S: Session,
    S::IoError: std::fmt::Debug,
{
    async fn frame(&mut self, id: FrameId, payload: &[u8]) -> Result<()> {
        let mut frame = Vec::with_capacity(5 + payload.len());

        frame.push(u8::from(id));
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload);

        self.channel.write(&frame).await
    }

    pub async fn write(&mut self, bytes: &[u8]) -> Result<()> {
        for part in bytes.chunks(MOST_INPUT) {
            self.frame(FrameId::Stdin, part).await?;
        }

        Ok(())
    }

    pub async fn close(&mut self) -> Result<()> {
        self.frame(FrameId::CloseStdin, &[]).await
    }

    pub async fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        let size = WindowSize { rows, cols }.encode();

        self.frame(FrameId::WindowSizeChange, size.as_bytes()).await
    }

    /// Closes the channel, which ends the program and any pending read of its
    /// output. This is the safe way to stop a reader, because dropping a read
    /// in progress can break the whole connection.
    pub async fn shut(self) -> Result<()> {
        self.channel.close().await
    }
}

impl<S> Output<S>
where
    S: Session,
    S::IoError: std::fmt::Debug,
{
    pub async fn next(&mut self) -> Result<Option<Said>> {
        loop {
            if let Some(said) = self.whole() {
                return Ok(Some(said));
            }

            if self.ended {
                return Ok(None);
            }

            let mut more = vec![0; 64 * 1024];
            let got = self.channel.read(&mut more).await?;

            if got == 0 {
                self.ended = true;
                continue;
            }

            self.pending.extend_from_slice(&more[..got]);
        }
    }

    fn whole(&mut self) -> Option<Said> {
        let said = said(&mut self.pending);

        if let Some(Said::Exit(_)) = said {
            self.ended = true;
            self.pending.clear();
        }

        said
    }
}

fn said(pending: &mut Vec<u8>) -> Option<Said> {
    loop {
        let header = pending.get(..5)?;
        let id = header[0];
        let length = u32::from_le_bytes([header[1], header[2], header[3], header[4]]) as usize;

        if pending.len() < 5 + length {
            return None;
        }

        let payload: Vec<u8> = pending.drain(..5 + length).skip(5).collect();

        match FrameId::try_from(id) {
            Ok(FrameId::Stdout) => return Some(Said::Stdout(payload)),
            Ok(FrameId::Stderr) => return Some(Said::Stderr(payload)),
            Ok(FrameId::Exit) => return Some(Said::Exit(payload.first().copied().unwrap_or(0))),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_frames_as_they_come() {
        let mut pending = Vec::new();

        pending.extend_from_slice(&[1, 2, 0, 0, 0, b'h', b'i']);
        pending.extend_from_slice(&[2, 1, 0, 0, 0, b'!']);
        pending.extend_from_slice(&[3, 1, 0, 0, 0, 7]);
        pending.extend_from_slice(&[1, 1, 0]);

        assert_eq!(said(&mut pending), Some(Said::Stdout(b"hi".to_vec())));
        assert_eq!(said(&mut pending), Some(Said::Stderr(b"!".to_vec())));
        assert_eq!(said(&mut pending), Some(Said::Exit(7)));
        assert_eq!(said(&mut pending), None);
        assert_eq!(pending, [1, 1, 0]);
    }
}
