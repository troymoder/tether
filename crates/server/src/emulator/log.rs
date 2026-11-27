use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use super::Emulators;

/// Enough to see why an emulator did not start.
const KEPT: usize = 64 * 1024;

impl Emulators {
    pub(super) fn log_of(&self, name: &str) -> Result<Arc<Mutex<String>>> {
        let list = self.list.lock().unwrap();
        let entry = list
            .iter()
            .find(|entry| entry.saved.name == name)
            .with_context(|| format!("emulator not found: {name}"))?;

        Ok(entry.log.clone())
    }

    pub(crate) fn log(&self, name: &str) -> Result<String> {
        Ok(self.log_of(name)?.lock().unwrap().clone())
    }
}

pub(super) fn keep(log: &Mutex<String>, line: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default();
    let mut log = log.lock().unwrap();

    // `Request::EmulatorLog` documents the form of a line.
    log.push_str(&format!("{now}\t{line}\n"));

    if let Some(excess) = log.len().checked_sub(KEPT) {
        let rest = &log.as_bytes()[excess..];
        let cut = rest
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(log.len(), |end| excess + end + 1);

        log.drain(..cut);
    }
}

pub(super) async fn follow(stream: impl AsyncRead + Unpin, log: Arc<Mutex<String>>) {
    let mut stream = BufReader::new(stream);
    let mut line = Vec::new();

    // Read as bytes because the emulator does not always print valid UTF-8.
    while let Ok(1..) = stream.read_until(b'\n', &mut line).await {
        keep(&log, String::from_utf8_lossy(&line).trim_end());
        line.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_end_of_the_log() {
        let log = Mutex::new("0\ta line\n".repeat(8 * 1024));

        keep(&log, "the end");

        let log = log.lock().unwrap();

        assert!(log.len() <= KEPT);
        assert!(log.starts_with("0\ta line\n"));
        assert!(log.ends_with("\tthe end\n"));
    }
}
