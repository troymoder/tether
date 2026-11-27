//! A [`Device`] is a handle on a connection that something else keeps
//! reading (libadb's `Reader::run`). Every operation opens its own channel,
//! so any number of them can run at once.

use workspace_hack as _;

mod channel;
mod process;

use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow, bail};
pub use libadb;
use libadb::Session;
use libadb::channel::ChannelId;
use libadb::shell::v2::{self, ShellMode};
use libadb::sync::{FileMode, HEADER_SIZE, SYNC_DATA_MAX};

pub use channel::Channel;
pub use process::{Input, Output, Process, Said, Terminal};

pub fn failed(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("adb failed: {error}")
}

pub(crate) fn broke<E: std::fmt::Debug>(error: libadb::Error<E>) -> anyhow::Error {
    match error {
        // A sync error holds the device's message, such as "open failed: No
        // such file or directory", which Debug would print as bytes.
        libadb::Error::Sync(error) => failed(error),
        error => failed(format!("{error:?}")),
    }
}

pub fn command<S: AsRef<str>>(parts: impl IntoIterator<Item = S>) -> Result<String> {
    let parts: Vec<S> = parts.into_iter().collect();

    shlex::try_join(parts.iter().map(AsRef::as_ref)).map_err(failed)
}

pub fn quoted(text: &str) -> Result<String> {
    shlex::try_quote(text)
        .map(std::borrow::Cow::into_owned)
        .map_err(failed)
}

pub struct Ran {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit: u8,
}

#[derive(Clone)]
pub struct Device<S> {
    session: S,
    /// Channels dropped without being closed. libadb frees a channel's slot
    /// only when it is closed, which has to be awaited, so these are closed
    /// before the next channel opens.
    dropped: Arc<Mutex<Vec<ChannelId>>>,
}

impl<S> Device<S>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    pub fn new(session: S) -> Self {
        Device {
            session,
            dropped: Arc::default(),
        }
    }

    pub fn session(&self) -> &S {
        &self.session
    }

    /// On a terminal, the program's stderr arrives as stdout.
    pub async fn spawn(&self, command: &str, terminal: Option<Terminal<'_>>) -> Result<Process<S>> {
        let mode = match terminal {
            Some(terminal) => ShellMode::Pty {
                term: Some(terminal.term),
            },
            None => ShellMode::Raw,
        };

        let destination =
            v2::destination(mode, command).map_err(|error| failed(format!("{error:?}")))?;
        // v2::destination ends with a NUL, which `channel` adds itself.
        let channel = self.channel(destination.trim_end_matches('\0')).await?;
        let mut process = Process::new(channel);

        if let Some(terminal) = terminal {
            process.input.resize(terminal.rows, terminal.cols).await?;
        }

        Ok(process)
    }

    pub async fn run(&self, command: &str, input: Option<&[u8]>) -> Result<Ran> {
        let Process {
            input: mut writer,
            mut output,
        } = self.spawn(command, None).await?;

        // Input is written while output is read. A program that prints a lot
        // before reading all its input would otherwise deadlock with us.
        let write = async {
            if let Some(input) = input {
                writer.write(input).await?;
            }

            writer.close().await
        };

        let read = async {
            let mut ran = Ran {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit: 0,
            };

            while let Some(said) = output.next().await? {
                match said {
                    Said::Stdout(bytes) => ran.stdout.extend_from_slice(&bytes),
                    Said::Stderr(bytes) => ran.stderr.extend_from_slice(&bytes),
                    Said::Exit(code) => ran.exit = code,
                }
            }

            Ok::<_, anyhow::Error>(ran)
        };

        // Both futures run to completion, because dropping one halfway
        // through a write would desynchronise the connection. A program that
        // exited with all its output read may have closed its channel before
        // its input was written, so a failed write is ignored.
        let (_, ran) = futures_util::future::join(write, read).await;
        let ran = ran?;

        let _ = writer.shut().await;

        Ok(ran)
    }

    pub async fn shell(&self, script: &str, input: Option<&[u8]>) -> Result<Vec<u8>> {
        let ran = self.run(script, input).await?;

        if ran.exit != 0 {
            let said = if ran.stderr.is_empty() {
                &ran.stdout
            } else {
                &ran.stderr
            };

            bail!("adb failed: {}", String::from_utf8_lossy(said).trim());
        }

        Ok(ran.stdout)
    }

    pub async fn script(&self, script: &str) -> Result<String> {
        let printed = self.shell(script, None).await?;

        Ok(String::from_utf8_lossy(&printed).into_owned())
    }

    pub async fn sh<P: AsRef<str>>(&self, parts: impl IntoIterator<Item = P>) -> Result<String> {
        self.script(&command(parts)?).await
    }

    pub async fn push(&self, file: &[u8], remote: &str) -> Result<()> {
        let mut session = self.session.clone();
        let mut buffer = vec![0; HEADER_SIZE + SYNC_DATA_MAX];
        let mut sync = libadb::sync::open(&mut session, &mut buffer)
            .await
            .map_err(broke)?;
        // Quit closes the sync channel, so it runs even after a failed send.
        let sent = sync.send(remote, FileMode(0o100_644), 0, file).await;
        let quit = sync.quit().await;

        sent.and(quit).map_err(broke)
    }

    pub async fn pull(&self, remote: &str) -> Result<Vec<u8>> {
        let mut session = self.session.clone();
        let mut buffer = vec![0; HEADER_SIZE + SYNC_DATA_MAX];
        let mut sync = libadb::sync::open(&mut session, &mut buffer)
            .await
            .map_err(broke)?;
        let file = sync.recv(remote).await;
        let quit = sync.quit().await;

        file.and_then(|file| quit.map(|()| file)).map_err(broke)
    }

    pub async fn channel(&self, service: &str) -> Result<Channel<S>> {
        let mut session = self.session.clone();
        let dropped = std::mem::take(&mut *self.dropped.lock().unwrap());

        for id in dropped {
            // A channel the device already closed fails here, which is fine.
            let _ = session.close_channel(id).await;
        }

        let destination = [service.as_bytes(), &[0]].concat();
        let id = session.open_channel(&destination).await.map_err(broke)?;

        Ok(Channel::new(session, id, self.dropped.clone()))
    }
}
