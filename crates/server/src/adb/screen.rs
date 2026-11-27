//! One scrcpy runs per device and source however many pages watch it, so the
//! device encodes once. A page that joins partway has scrcpy reset its video so
//! the page starts on a key frame.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use android::scrcpy::{self, Input, Picture, control_message};
use anyhow::{Context, Result, anyhow};
use protocol::{Screen, encode};
use tokio::sync::{broadcast, mpsc};

use super::host::Session;
use crate::app::App;
use crate::media::{Part, Watching, profile};
use crate::rtc::{Sample, Track};

/// About a second of frames and sound, kept for a page that falls behind before
/// it skips to the next key frame.
const BACKLOG: usize = 256;

/// A still screen sends no frames, so a running scrcpy checks at this interval
/// that a page still watches it.
const IDLE: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(crate) struct Screens {
    running: Mutex<HashMap<String, Arc<Share>>>,
}

/// A camera's zoom is left out of the key, as it only says where the first page
/// started, and the camera can stream at one size at a time anyway.
fn key(device: &str, source: &scrcpy::Source) -> String {
    match source {
        scrcpy::Source::Screen => device.to_owned(),
        scrcpy::Source::Camera { id, mode, .. } => format!("{device} camera {id} {mode:?}"),
    }
}

struct Share {
    parts: broadcast::Sender<Part>,
    /// An encoded [`Screen::Codec`], sent first to a page that joins later.
    codec: Mutex<Option<Arc<Vec<u8>>>>,
    control: mpsc::UnboundedSender<Vec<u8>>,
}

impl Screens {
    pub(crate) fn watch(
        &self,
        app: &Arc<App>,
        device: &str,
        source: scrcpy::Source,
    ) -> Result<Watching> {
        let adb = app.adb.device(device)?;
        let key = key(device, &source);
        let mut running = self.running.lock().unwrap();
        let share = running
            .entry(key.clone())
            .or_insert_with(|| {
                let (control, controls) = mpsc::unbounded_channel();
                let share = Arc::new(Share {
                    parts: broadcast::channel(BACKLOG).0,
                    codec: Mutex::new(None),
                    control,
                });

                tokio::spawn(run(
                    app.clone(),
                    key,
                    (adb, source),
                    share.clone(),
                    controls,
                ));

                share
            })
            .clone();
        let codec = share.codec.lock().unwrap().clone();
        let parts = share.parts.subscribe();

        // Queued until scrcpy's control socket is up. A new scrcpy starts with
        // a key frame anyway, so the reset costs nothing there.
        let _ = share.control.send(control_message(&Input::ResetVideo));

        Ok(Watching {
            codec,
            kept: None,
            parts,
            control: Some(share.control.clone()),
        })
    }

    /// Runs under the same lock as [`Screens::watch`], so a page that comes
    /// meanwhile either keeps the scrcpy or starts a new one.
    fn forget(&self, key: &str, share: &Arc<Share>, ended: bool) -> bool {
        let mut running = self.running.lock().unwrap();

        if !ended && share.parts.receiver_count() > 0 {
            return false;
        }

        if running
            .get(key)
            .is_some_and(|running| Arc::ptr_eq(running, share))
        {
            running.remove(key);
        }

        true
    }
}

impl Share {
    fn send(&self, sample: Sample) {
        // No page may be subscribed for a moment, which loses nothing.
        let _ = self.parts.send(Part::Sample(Arc::new(sample)));
    }

    fn codec(&self, codec: String) {
        let message = Arc::new(encode(&Screen::Codec(codec)));

        *self.codec.lock().unwrap() = Some(message.clone());
        let _ = self.parts.send(Part::Told(message));
    }
}

/// The scrcpy is forgotten before the pages are told why it failed, so a page
/// that tries again starts a new one.
async fn run(
    app: Arc<App>,
    key: String,
    (adb, source): (adb::Device<Session>, scrcpy::Source),
    share: Arc<Share>,
    controls: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let ran = capture(&app, &key, (&adb, source), &share, controls).await;

    app.screens.forget(&key, &share, true);

    if let Err(error) = ran {
        let error = format!("{error:#}");

        tracing::info!("{key}: {error}");

        let _ = share
            .parts
            .send(Part::Failed(Arc::new(encode(&Screen::Failed(error)))));
    }
}

async fn capture(
    app: &Arc<App>,
    key: &str,
    (adb, source): (&adb::Device<Session>, scrcpy::Source),
    share: &Arc<Share>,
    mut controls: mpsc::UnboundedReceiver<Vec<u8>>,
) -> Result<()> {
    let jar = tokio::fs::read(&app.scrcpy_server)
        .await
        .with_context(|| format!("scrcpy failed: {}", app.scrcpy_server.display()))?;
    // Names scrcpy's socket, apart from any other scrcpy on the device.
    let id = getrandom::u32().map_err(|error| anyhow!("scrcpy failed: {error}"))?;
    let scrcpy::Screen {
        mut pictures,
        sound,
        mut replies,
        mut control,
        mut server,
        closer,
    } = scrcpy::start(adb, &jar, source, id, tokio::time::sleep).await?;
    // Each socket is read in a task of its own, as a read cut short would
    // lose its place in the stream. Each tells here when it ended.
    let (ended, mut end) = mpsc::unbounded_channel::<Result<()>>();
    let mut tasks = Vec::new();

    {
        let share = share.clone();
        let ended = ended.clone();

        tasks.push(tokio::spawn(async move {
            let mut shown = 0x64;
            let read = loop {
                match pictures.next().await {
                    Ok(Some(Picture::Codec(codec))) => {
                        shown = profile(&codec);
                        share.codec(codec);
                    }
                    Ok(Some(Picture::Frame { key, time, data })) => share.send(Sample {
                        track: Track::Video { profile: shown },
                        time,
                        key,
                        data: data.into(),
                    }),
                    Ok(None) => break Err(anyhow!("screen failed: scrcpy stopped")),
                    Err(error) => break Err(error),
                }
            };

            let _ = ended.send(read);
        }));
    }

    // A device without sound to give ends this at once, and the screen
    // goes on without it.
    if let Some(mut sound) = sound {
        let share = share.clone();

        tasks.push(tokio::spawn(async move {
            while let Ok(Some((time, data))) = sound.next().await {
                share.send(Sample {
                    track: Track::Audio,
                    time,
                    key: true,
                    data: data.into(),
                });
            }
        }));
    }

    {
        let share = share.clone();
        let ended = ended.clone();

        tasks.push(tokio::spawn(async move {
            while let Ok(Some(text)) = replies.next().await {
                let told = Arc::new(encode(&Screen::Clipboard(text)));

                let _ = share.parts.send(Part::Told(told));
            }

            let _ = ended.send(Err(anyhow!("screen failed: scrcpy stopped")));
        }));
    }

    // scrcpy's output says why it stopped, such as a camera in use or
    // Android older than 12.
    tasks.push(tokio::spawn(async move {
        let mut printed = String::new();

        while let Ok(Some(said)) = server.next().await {
            if let adb::Said::Stdout(bytes) | adb::Said::Stderr(bytes) = said {
                printed.push_str(&String::from_utf8_lossy(&bytes));
            }
        }

        let _ = ended.send(Err(anyhow!("screen failed: {}", stopped(&printed))));
    }));

    let mut idle = tokio::time::interval(IDLE);
    let ran = loop {
        tokio::select! {
            Some(ran) = end.recv() => break ran,
            Some(message) = controls.recv() => {
                if let Err(error) = control.write(&message).await {
                    break Err(error);
                }
            }
            _ = idle.tick() => {
                if app.screens.forget(key, share, false) {
                    break Ok(());
                }
            }
        }
    };

    for task in tasks {
        task.abort();
    }

    let _ = closer.close().await;

    ran
}

/// scrcpy prints its errors as `[server] ERROR: …`.
fn stopped(printed: &str) -> &str {
    printed
        .lines()
        .rev()
        .find_map(|line| line.split_once("ERROR:"))
        .map_or("scrcpy stopped", |(_, why)| why.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_why_scrcpy_stopped() {
        let printed = "[server] INFO: Device: Pixel\n[server] ERROR: Camera 0 in use\n";

        assert_eq!(stopped(printed), "Camera 0 in use");
        assert_eq!(stopped("[server] INFO: done\n"), "scrcpy stopped");
    }

    #[test]
    fn a_camera_is_shared_whatever_its_zoom() {
        let camera = |zoom| scrcpy::Source::Camera {
            id: "0".to_owned(),
            mode: None,
            zoom,
        };

        assert_eq!(key("a", &camera(1.0)), key("a", &camera(2.0)));
        assert_ne!(key("a", &camera(1.0)), key("a", &scrcpy::Source::Screen));
    }
}
