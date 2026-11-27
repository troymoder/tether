//! One capture runs per phone however many pages watch it. It starts when the
//! phone is connected and runs until it is disconnected or unplugged, whether
//! pages watch or not. A second session on the phone needs it switched off and
//! on again (iOS 27.3), so a page that comes later joins the running one.
//!
//! Everything that switches a phone or captures it takes the phone's turn,
//! since the phone allows one at a time.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, bail};
use protocol::{Screen, encode};
use tokio::sync::{Notify, broadcast};

use super::sound::Sound;
use super::{Iphones, Shared};
use crate::media::{Part, Watching, profile};
use crate::rtc::{Sample, Track};

/// About a second of frames and sound, kept for a page that falls behind before
/// it skips to the next key frame.
const BACKLOG: usize = 256;

#[derive(Default)]
pub(super) struct Screens {
    running: Mutex<HashMap<String, Arc<Share>>>,
    /// One lock per phone, held by its capture from start to end.
    turns: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// The time each phone was last switched off and on for a start that got
    /// no answer.
    cycled: Mutex<HashMap<String, std::time::Instant>>,
}

struct Share {
    parts: broadcast::Sender<Part>,
    /// A black frame while the phone's screen is off, which a page that
    /// comes later starts with. Taken together with a subscription, so none
    /// is missed between the two.
    kept: Mutex<Option<Arc<Sample>>>,
    /// The video's codec as an encoded [`Screen::Codec`], which a page that
    /// comes later is told first. Taken with a subscription too.
    codec: Mutex<Option<Arc<Vec<u8>>>>,
    stop: Notify,
}

impl Iphones {
    pub(crate) fn watch(&self, udid: &str) -> Result<Watching> {
        if !self.holds(udid) {
            bail!("screen failed: {} not connected", super::mention(udid));
        }

        let share = self.0.screens.begin(&self.0, udid);
        let codec = share.codec.lock().unwrap();
        let kept = share.kept.lock().unwrap();

        Ok(Watching {
            codec: codec.clone(),
            kept: kept.clone(),
            parts: share.parts.subscribe(),
            control: None,
        })
    }
}

impl Screens {
    pub(super) fn start(&self, shared: &Arc<Shared>, udid: &str) {
        self.begin(shared, udid);
    }

    fn begin(&self, shared: &Arc<Shared>, udid: &str) -> Arc<Share> {
        self.running
            .lock()
            .unwrap()
            .entry(udid.to_owned())
            .or_insert_with(|| {
                let share = Arc::new(Share {
                    parts: broadcast::channel(BACKLOG).0,
                    kept: Mutex::new(None),
                    codec: Mutex::new(None),
                    stop: Notify::new(),
                });

                tokio::spawn(capture(shared.clone(), udid.to_owned(), share.clone()));

                share
            })
            .clone()
    }

    pub(super) fn end(&self, udid: &str) {
        if let Some(share) = self.running.lock().unwrap().remove(udid) {
            share.stop.notify_one();
        }
    }

    fn forget(&self, udid: &str, share: &Arc<Share>) {
        let mut running = self.running.lock().unwrap();

        if running
            .get(udid)
            .is_some_and(|running| Arc::ptr_eq(running, share))
        {
            running.remove(udid);
        }
    }

    pub(super) fn turn(&self, udid: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.turns
            .lock()
            .unwrap()
            .entry(udid.to_owned())
            .or_default()
            .clone()
    }
}

impl Share {
    fn send(&self, sample: Sample) {
        // No page may be subscribed for a moment, which loses nothing.
        let _ = self.parts.send(Part::Sample(Arc::new(sample)));
    }

    fn codec(&self, codec: String) {
        let mut kept = self.codec.lock().unwrap();
        let message = Arc::new(encode(&Screen::Codec(codec)));

        *kept = Some(message.clone());
        let _ = self.parts.send(Part::Told(message));
    }

    fn fail(&self, error: String) {
        let _ = self
            .parts
            .send(Part::Failed(Arc::new(encode(&Screen::Failed(error)))));
    }

    fn black(&self, frame: Option<Sample>) {
        let mut kept = self.kept.lock().unwrap();

        *kept = frame.map(Arc::new);

        if let Some(frame) = &*kept {
            let _ = self.parts.send(Part::Sample(frame.clone()));
        }
    }
}

fn video(profile: u8, key: bool, time: u64, data: Vec<u8>) -> Sample {
    Sample {
        track: Track::Video { profile },
        time,
        key,
        data: data.into(),
    }
}

/// A phone is not switched again this soon after a switch for a start that got
/// no answer. A page retries a failed screen a few times, and a phone switched
/// off and on for each retry stops answering altogether until it is restarted
/// (seen on iOS 27.3, 2026-11-24).
const CYCLE_AGAIN: Duration = Duration::from_secs(60);

const PAIRING: Duration = Duration::from_secs(30);

/// The phone needs tetherd to hold it paired over usbmux before it streams (iOS
/// 27.3): every capture that worked had usbmux open, and one started while
/// tetherd kept it closed never streamed. A phone stuck after an earlier
/// session is switched off and on once, which it drops off the bus for, and
/// then waited for again.
async fn start(
    shared: &Arc<Shared>,
    udid: &str,
    phone: &quicktime::Phone,
) -> Result<quicktime::Capture> {
    paired(shared, udid).await?;

    let cycled_lately = shared
        .screens
        .cycled
        .lock()
        .unwrap()
        .get(udid)
        .is_some_and(|cycled| cycled.elapsed() < CYCLE_AGAIN);

    // Without a switch in reach, a start just waits longer for the phone.
    let within = if cycled_lately {
        quicktime::STARTING
    } else {
        quicktime::WARM
    };

    match quicktime::Capture::start(phone, within).await {
        Ok(capture) => return Ok(capture),
        Err(error) if cycled_lately => return Err(error),
        Err(_) => {}
    }

    shared
        .screens
        .cycled
        .lock()
        .unwrap()
        .insert(udid.to_owned(), std::time::Instant::now());
    tracing::info!("{udid}: the phone did not answer, switching it again");
    shared.switching(udid, true);

    let cycled = quicktime::cycle(phone).await;

    shared.switching(udid, false);
    cycled?;
    paired(shared, udid).await?;
    quicktime::Capture::start(phone, quicktime::STARTING).await
}

async fn paired(shared: &Shared, udid: &str) -> Result<()> {
    let waited = tokio::time::timeout(PAIRING, async {
        while !shared.paired(udid) {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await;

    anyhow::ensure!(
        waited.is_ok(),
        "screen failed: {} never paired",
        super::mention(udid)
    );

    Ok(())
}

/// None if the phone does not let its screen be watched, when the page keeps
/// its last frame.
async fn display(shared: &Arc<Shared>, udid: &str) -> Option<ios::display::Display> {
    let phone = Iphones(shared.clone()).phone(udid).await.ok()?;

    match ios::display::Display::watch(&phone).await {
        Ok(display) => Some(display),
        Err(error) => {
            tracing::debug!("{udid}: {error:#}");

            None
        }
    }
}

/// The time a capture may go without a frame before the phone's screen is
/// taken to be off. With it on, frames come at once.
const DARK: Duration = Duration::from_millis(1500);

/// Only for a screen that is off: on one that is on, the press turns it off.
/// The HID service needs Developer Mode and the developer disk image, and the
/// phone took input only while CoreDevice's display stream ran, so this may do
/// nothing (`ios::input`).
async fn wake(shared: Arc<Shared>, udid: String) {
    let pressed = async {
        let phone = Iphones(shared).phone(&udid).await?;
        let operation = protocol::IosOperation::Open {
            service: ios::input::TUNNEL.to_owned(),
        };
        let ios::perform::Done::Stream(tunnel) = ios::perform::perform(&phone, &operation).await?
        else {
            anyhow::bail!("input failed: no tunnel");
        };
        let mut input = ios::input::Input::open(tunnel).await?;

        input.button(ios::input::Button::Power, true).await?;
        input.button(ios::input::Button::Power, false).await
    };

    match pressed.await {
        Ok(()) => tracing::info!("{udid}: pressed power to wake the screen"),
        Err(error) => tracing::info!("{udid}: could not wake the screen: {error:#}"),
    }
}

/// Until its first unlock since it started, a phone opens no notification
/// proxy, so a watch of the screen is tried again at this interval.
const WATCH_AGAIN: Duration = Duration::from_secs(5);

/// The watch runs in a task of its own because the capture's loop drops what it
/// waits on for every frame, which would cut an opening or a notification
/// short.
struct Watch {
    changes: tokio::sync::mpsc::UnboundedReceiver<()>,
    task: tokio::task::JoinHandle<()>,
}

impl Watch {
    fn open(shared: &Arc<Shared>, udid: &str) -> Self {
        let shared = shared.clone();
        let udid = udid.to_owned();
        let (sender, changes) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            loop {
                if let Some(mut display) = display(&shared, &udid).await {
                    while display.changed().await.is_ok() {
                        if sender.send(()).is_err() {
                            return;
                        }
                    }
                }

                tokio::time::sleep(WATCH_AGAIN).await;
            }
        });

        Self { changes, task }
    }

    async fn changed(&mut self) {
        if self.changes.recv().await.is_none() {
            std::future::pending::<()>().await;
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The direction the phone's screen changed in is read from its frames: one
/// that is on sends a frame every 16 ms, and one that is off none at all. A
/// screen that wakes sends its first frames just before the change is told (iOS
/// 27.3), so frames alone would take a wake for the screen going off. It went
/// off only if frames came lately and had been coming without a gap for longer
/// than [`LIT`].
#[derive(Default)]
struct Frames {
    last: Option<std::time::Instant>,
    /// The time frames came again after a gap of [`GAP`] or more.
    since: Option<std::time::Instant>,
}

/// A gap between frames this long, with the screen on, is logged as a
/// freeze. A screen that is on sends a frame every 16 ms.
const STALL: Duration = Duration::from_millis(250);

/// The time frames must have been coming for the screen to have been on.
const LIT: Duration = Duration::from_secs(1);

/// A pause in the frames this long means the screen was off.
const GAP: Duration = Duration::from_millis(500);

impl Frames {
    fn came(&mut self, now: std::time::Instant) {
        if self.last.is_none_or(|last| now - last >= GAP) {
            self.since = Some(now);
        }

        self.last = Some(now);
    }

    fn went_off(&self, now: std::time::Instant) -> bool {
        self.last.is_some_and(|last| now - last < GAP)
            && self.since.is_some_and(|since| now - since >= LIT)
    }
}

async fn capture(shared: Arc<Shared>, udid: String, share: Arc<Share>) {
    let turn = shared.screens.turn(&udid);
    let _turn = turn.lock().await;
    let phone = quicktime::Phone::new(&udid);
    let started = tokio::select! {
        started = start(&shared, &udid, &phone) => Some(started),
        () = share.stop.notified() => None,
    };

    let mut capture = match started {
        None => return,
        Some(Ok(capture)) => {
            tracing::info!("{udid}: screen capture started");

            capture
        }
        Some(Err(error)) => {
            tracing::warn!("{udid}: screen failed: {error:#}");
            share.fail(format!("screen failed: {error:#}"));
            shared.screens.forget(&udid, &share);

            return;
        }
    };

    let mut sound = match Sound::new() {
        Ok(sound) => Some(sound),
        Err(error) => {
            tracing::warn!("{udid}: {error:#}");

            None
        }
    };
    // The video's `profile_idc`, from its codec, which picks the browser's
    // payload type for it.
    let mut shown_profile = 0x64;
    let mut watch = Watch::open(&shared, &udid);
    let mut blank = false;
    let mut frames = Frames::default();
    // The last frame's arrival and its time on the phone's clock.
    let mut previous: Option<(std::time::Instant, u64)> = None;
    // Until the first frame: a phone that sends none soon has its screen
    // off, and is woken once.
    let mut dark = Some(Box::pin(tokio::time::sleep(DARK)));

    loop {
        let media = tokio::select! {
            media = capture.next() => media,
            () = async { dark.as_mut().expect("checked").await }, if dark.is_some() => {
                dark = None;

                // On a thread of its own, since the HID client's future is
                // not Send: idevice's RsdService is only for a 'static
                // stream, which the compiler cannot see through.
                let shared = shared.clone();
                let udid = udid.clone();
                let runtime = tokio::runtime::Handle::current();

                tokio::task::spawn_blocking(move || runtime.block_on(wake(shared, udid)));

                continue;
            }
            () = watch.changed() => {
                // It went off if frames were coming. Coming on needs
                // nothing: the phone's next key frame shows it again.
                if frames.went_off(std::time::Instant::now())
                    && let Some(quicktime::Media::Video { key, time, data }) = capture.black()
                {
                    blank = true;
                    share.black(Some(video(shown_profile, key, time, data)));
                }

                continue;
            }
            () = share.stop.notified() => break,
        };

        match media {
            Ok(quicktime::Media::Codec(codec)) => {
                shown_profile = profile(&codec);
                share.codec(codec);
            }
            Ok(quicktime::Media::Video { key, time, data }) => {
                let now = std::time::Instant::now();

                // The source of a freeze: a phone that made no frames
                // moves its clock on as much as the gap, and frames that
                // were held up on the way come late with their clock as it
                // was.
                if !blank
                    && let Some((arrived, shown)) = previous
                    && now - arrived >= STALL
                {
                    tracing::info!(
                        "{udid}: no frame for {} ms, the phone's clock moved {} ms",
                        (now - arrived).as_millis(),
                        time.saturating_sub(shown) / 1000
                    );
                }

                previous = Some((now, time));
                dark = None;
                frames.came(now);

                if blank {
                    blank = false;
                    share.black(None);
                }

                share.send(video(shown_profile, key, time, data));
            }
            Ok(quicktime::Media::Sound { time, data }) => {
                for (time, packet) in sound
                    .as_mut()
                    .map(|sound| sound.encode(time, &data))
                    .unwrap_or_default()
                {
                    share.send(Sample {
                        track: Track::Audio,
                        time,
                        key: true,
                        data: packet.into(),
                    });
                }
            }
            Err(error) => {
                tracing::warn!("{udid}: screen failed: {error:#}");
                share.fail(format!("screen failed: {error:#}"));
                shared.screens.forget(&udid, &share);

                break;
            }
        }
    }

    capture.stop().await;
    tracing::info!("{udid}: screen capture stopped");
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn stream(frames: &mut Frames, from: Instant, long: Duration) -> Instant {
        let mut at = from;

        while at < from + long {
            frames.came(at);
            at += Duration::from_millis(16);
        }

        at
    }

    #[test]
    fn a_change_after_steady_frames_is_the_screen_going_off() {
        let mut frames = Frames::default();
        let now = stream(&mut frames, Instant::now(), Duration::from_secs(3));

        assert!(frames.went_off(now));
    }

    #[test]
    fn a_change_just_after_frames_came_back_is_a_wake() {
        let mut frames = Frames::default();
        let start = Instant::now();
        let slept = stream(&mut frames, start, Duration::from_secs(3));
        // Off for 5 s, then the wake's first frames just before it is told.
        let now = stream(
            &mut frames,
            slept + Duration::from_secs(5),
            Duration::from_millis(100),
        );

        assert!(!frames.went_off(now));
        // With no frames at all lately, a change is a wake too.
        assert!(!frames.went_off(slept + Duration::from_secs(2)));
    }
}
