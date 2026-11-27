use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};

use crate::camera::Fed;
use crate::task::Stopped;

pub(crate) struct Decoder {
    fed: Arc<Fed>,
    size: (usize, usize),
    input: ChildStdin,
    _ffmpeg: Child,
    _reading: Stopped,
}

impl Decoder {
    pub(crate) fn start(ffmpeg: &Path, fed: Arc<Fed>, size: (usize, usize)) -> Result<Self> {
        let (width, height) = size;
        let mut ffmpeg = Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            // Each frame goes out as soon as it is in, with no lookahead and
            // no frame threads that each hold a frame. `-fflags nobuffer` is
            // left out because ffmpeg 9 gives no output with it.
            .args(["-flags", "low_delay", "-thread_type", "slice"])
            .args(["-probesize", "32", "-analyzeduration", "0"])
            .args(["-f", "h264", "-i", "pipe:0"])
            // Writes each frame out whole as soon as it is decoded.
            .args(["-flush_packets", "1"])
            .args(["-f", "rawvideo", "-pix_fmt", "yuv420p"])
            .args(["-s", &format!("{width}x{height}"), "pipe:1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("{}", ffmpeg.display()))?;

        let input = ffmpeg.stdin.take().context("ffmpeg: no input")?;
        let mut output = ffmpeg.stdout.take().context("ffmpeg: no output")?;
        let camera = fed.clone();
        let reading = tokio::spawn(async move {
            let mut frame = vec![0; width * height * 3 / 2];

            while output.read_exact(&mut frame).await.is_ok() {
                camera.raw(frame.clone());
            }
        });

        Ok(Self {
            fed,
            size,
            input,
            _ffmpeg: ffmpeg,
            _reading: Stopped(reading.abort_handle()),
        })
    }

    pub(crate) fn feeds(&self, fed: &Arc<Fed>, size: (usize, usize)) -> bool {
        Arc::ptr_eq(&self.fed, fed) && self.size == size
    }

    // shortcut: what the page sends waits unbounded meanwhile. The page drops
    // frames once its channel backs up, which keeps the wait short.
    pub(crate) async fn give(&mut self, video: &[u8]) {
        if let Err(error) = self.input.write_all(video).await {
            tracing::error!("camera failed: ffmpeg: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Needs ffmpeg with libx264 on the PATH, which makes the video too.
    #[tokio::test]
    async fn decodes_video_into_the_camera() {
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error"])
            .args(["-f", "lavfi", "-i", "testsrc=size=64x48:rate=10"])
            // Encoded as the page does it: baseline, with no frame that
            // waits for a later one.
            .args(["-frames:v", "5", "-pix_fmt", "yuv420p", "-c:v", "libx264"])
            .args(["-profile:v", "baseline", "-tune", "zerolatency"])
            .args(["-f", "h264", "pipe:1"])
            .output();
        let Some(made) = made.ok().filter(|made| made.status.success()) else {
            eprintln!("skipped: no ffmpeg with libx264");
            return;
        };

        let fed = Arc::new(Fed::start(64, 48));
        let mut frames = fed.frames();
        let mut decoder = Decoder::start(Path::new("ffmpeg"), fed.clone(), (64, 48)).unwrap();

        assert!(decoder.feeds(&fed, (64, 48)));
        assert!(!decoder.feeds(&fed, (32, 24)));

        decoder.give(&made.stdout).await;
        tokio::time::timeout(Duration::from_secs(10), frames.changed())
            .await
            .unwrap()
            .unwrap();

        // The test picture has bright bars, so a black frame fails this.
        let frame = frames.borrow().clone();

        assert_eq!(frame.len(), 64 * 48 * 3 / 2);
        assert!(frame[..64 * 48].iter().any(|&light| light > 100));
    }
}
