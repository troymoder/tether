//! The page makes the media channel and transfers it to the worker right
//! away, because a data channel can only be transferred in the task that
//! created it.

use std::cell::Cell;
use std::rc::Rc;

use js_sys::{Array, Reflect, Uint8Array};
use leptos::prelude::*;
use protocol::{CameraHeld, CameraSettings, Framing, Lens, Media, Status, access, encode, own};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    HtmlImageElement, HtmlVideoElement, MediaStream, MediaStreamTrack, MessageEvent, VideoFrame,
    VideoFrameInit, Worker,
};

use super::{Feed, object};
use crate::link::Link;

#[wasm_bindgen]
extern "C" {
    /// Starts the page's own WebAssembly again in a worker, which then runs
    /// `worker::capture_worker` (glue/worker/index.ts).
    #[wasm_bindgen(catch, js_namespace = tetherCapture, js_name = start)]
    fn start_worker(module: &JsValue) -> Result<Worker, JsValue>;
}

fn hand_over(link: Link, worker: &Worker, emulator: &str, lens: Lens, lossless: bool) -> bool {
    let Some(pc) = link.peer() else {
        return false;
    };

    let channel = pc.create_data_channel(protocol::MEDIA);
    let first = encode(&Media::Camera {
        emulator: emulator.to_owned(),
        lens,
        lossless,
    });
    let message = object(&[
        ("channel", channel.clone().into()),
        ("first", Uint8Array::from(first.as_slice()).into()),
    ]);

    if worker
        .post_message_with_transfer(&message, &Array::of1(&channel))
        .is_err()
    {
        channel.close();

        return false;
    }

    true
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sending {
    pub pictures: u32,
    /// The average bytes of one packed frame.
    pub packed: u32,
    /// The average milliseconds the worker took over one frame.
    pub took: f64,
}

#[derive(Clone)]
pub(super) enum Player {
    Video(HtmlVideoElement),
    Image(HtmlImageElement),
}

impl Player {
    fn frame(&self) -> Option<VideoFrame> {
        match self {
            Player::Video(video) if video.video_width() > 0 => {
                VideoFrame::new_with_html_video_element(video).ok()
            }
            Player::Image(image) if image.complete() && image.natural_width() > 0 => {
                // A frame needs a timestamp, which a still image lacks.
                let init = VideoFrameInit::new();

                init.set_timestamp(0);

                VideoFrame::new_with_html_image_element_and_video_frame_init(image, &init).ok()
            }
            _ => None,
        }
    }
}

/// The source itself, a stream or a file, is held by `Feeding` beside it.
pub(super) struct Running {
    lens: Lens,
    player: Player,
    ticking: IntervalHandle,
    /// Holds the media channel the frames go on. Closing the channel
    /// releases the camera, which tetherd then sends black.
    worker: Worker,
    _ended: Closure<dyn Fn()>,
    _broke: Closure<dyn Fn()>,
    _packed: Closure<dyn Fn(MessageEvent)>,
}

pub(super) fn set_lossless(feed: Feed, on: bool) {
    feed.lossless.set(on);

    let Ok(emulator) = feed.emulator() else {
        return;
    };

    feed.feeding.try_with_untracked(|feeding| {
        if let Some(running) = feeding.running() {
            hand_over(feed.link, &running.worker, &emulator, running.lens, on);
        }
    });
}

fn heed(feed: Feed, said: &[u8]) {
    let Ok(held) = access::<CameraHeld>(said).map(own::<CameraHeld>) else {
        return;
    };

    let sending = feed
        .feeding
        .try_with_untracked(|feeding| feeding.running().is_some());

    if sending != Some(true) {
        return;
    }

    feed.stop();

    if held.taken {
        feed.lost_camera.set(Some(held.by));
    } else {
        feed.link.did(
            "camera",
            feed.device().as_ref(),
            Err(format!("camera failed: in use by {}", held.by.name)),
        );
    }
}

/// The time a worker told to close its channel gets before it is terminated.
const LETTING_GO: std::time::Duration = std::time::Duration::from_secs(1);

impl Running {
    pub(super) fn end(self) {
        self.ticking.clear();
        self.worker.set_onmessage(None);

        let _ = self.worker.post_message(&object(&[("close", true.into())]));
        let worker = self.worker.clone();

        set_timeout(move || worker.terminate(), LETTING_GO);

        match &self.player {
            Player::Video(video) => {
                video.pause().ok();
                video.set_src_object(None);
                let _ = video.remove_attribute("src");
            }
            Player::Image(image) => {
                let _ = image.remove_attribute("src");
            }
        }
    }
}

/// The camera's settings, and true if the emulator is upright.
fn camera_of(link: Link, emulator: &str, lens: Lens) -> Option<(CameraSettings, bool)> {
    untrack(|| {
        let run = link
            .emulator(emulator)?
            .emulator
            .filter(|run| run.status != Status::Shutdown)?;
        let settings = link.settings(emulator)?;

        Some((settings.camera(lens)?, run.upright))
    })
}

/// `captured` is None for a file. The caller holds the stream or the file and
/// releases it if this fails.
pub(super) fn run(
    feed: Feed,
    lens: Lens,
    player: Player,
    captured: Option<&MediaStream>,
) -> Result<Running, String> {
    let link = feed.link;
    let emulator = feed.emulator()?;

    feed.stop();
    feed.frame(Framing::default());

    let Some((settings, _)) = camera_of(link, &emulator, lens) else {
        return Err("camera failed: emulator not running".to_owned());
    };

    if link.peer().is_none() {
        return Err("capture failed: not connected to the server".to_owned());
    }

    let Ok(worker) = start_worker(&wasm_bindgen::module()) else {
        return Err("capture failed: no worker".to_owned());
    };

    // True while the worker has a frame to work on. It starts true until the
    // worker reports that it has loaded.
    let busy = Rc::new(Cell::new(true));
    let sent = Rc::new(Cell::new(js_sys::Date::now()));
    let done = busy.clone();
    let sent_at = sent.clone();
    let loaded = Cell::new(false);
    let named = emulator.clone();
    let handed = worker.clone();
    // The tally since the statistics were last reported: the time of that
    // report, the frame count, their bytes, and the time the worker took.
    let tally = Cell::new((js_sys::Date::now(), 0u32, 0u32, 0.0));
    let packed = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
        let answer = event.data();
        let part = |key: &str| Reflect::get(&answer, &key.into()).ok();

        // The worker's first message says it has loaded and is listening, so
        // the channel can be transferred now.
        if !loaded.replace(true) {
            let lossless = feed.lossless.try_get_untracked().unwrap_or(false);

            if !hand_over(link, &handed, &named, lens, lossless) {
                link.did(
                    "camera",
                    feed.device().as_ref(),
                    Err("capture failed: not connected to the server".to_owned()),
                );
                feed.stop();

                return;
            }

            done.set(false);

            return;
        }

        if part("done").is_some_and(|done| done.is_truthy()) {
            done.set(false);

            return;
        }

        if let Some(said) = part("held").filter(|said| !said.is_undefined()) {
            heed(feed, &Uint8Array::new(&said).to_vec());

            return;
        }

        if let Some(bytes) = part("bytes").and_then(|bytes| bytes.as_f64()) {
            let took = part("took").and_then(|took| took.as_f64()).unwrap_or(0.0);
            let (since, pictures, packed, taken) = tally.get();
            let pictures = pictures + 1;
            let packed = packed + bytes as u32;
            let now = js_sys::Date::now();

            sent_at.set(now);

            if now - since >= 1000.0 {
                feed.sending.try_set(Some(Sending {
                    pictures,
                    packed: packed / pictures,
                    took: (taken + took) / f64::from(pictures),
                }));

                tally.set((now, 0, 0, 0.0));
            } else {
                tally.set((since, pictures, packed, taken + took));
            }
        }
    });

    worker.set_onmessage(Some(packed.as_ref().unchecked_ref()));

    let sender = worker.clone();
    let playing = player.clone();
    let tick = move || {
        // The emulator stopped, or has no camera for that lens.
        let Some((settings, upright)) = camera_of(link, &emulator, lens) else {
            feed.stop();

            return;
        };

        if js_sys::Date::now() - sent.get() > 2000.0
            && feed.sending.try_with_untracked(Option::is_some) == Some(true)
        {
            feed.sending.try_set(None);
        }

        // Skips this tick while the previous frame is still in the worker.
        // The worker also skips frames while too much is queued to send.
        if busy.get() {
            return;
        }

        let Some(frame) = playing.frame() else {
            return;
        };

        let Framing {
            zoom,
            x,
            y,
            quarters,
            mirrored,
        } = feed.framing.try_get_untracked().unwrap_or_default();
        let lossless = feed.lossless.try_get_untracked().unwrap_or(false);
        let asked = object(&[
            ("width", settings.width.into()),
            ("height", settings.height.into()),
            ("fps", settings.fps.into()),
            ("zoom", zoom.into()),
            ("x", x.into()),
            ("y", y.into()),
            ("quarters", quarters.into()),
            ("upright", upright.into()),
            ("mirrored", mirrored.into()),
            ("lossless", lossless.into()),
            // A clone of the JS handle. `VideoFrame::clone` would make a new frame.
            ("frame", AsRef::<JsValue>::as_ref(&frame).clone()),
        ]);

        if sender
            .post_message_with_transfer(&asked, &Array::of1(&frame))
            .is_ok()
        {
            busy.set(true);
        } else {
            frame.close();
        }
    };

    let every = std::time::Duration::from_millis(1000 / u64::from(settings.fps.max(1)));

    let Ok(ticking) = set_interval_with_handle(tick, every) else {
        worker.terminate();

        return Err("capture failed: no timer".to_owned());
    };

    // The browser has its own control to stop sharing a screen, which ends
    // the tracks. A file it cannot play is reported once.
    let failed = captured.is_none();
    let ended = Closure::<dyn Fn()>::new(move || {
        if failed {
            link.did(
                "camera",
                feed.device().as_ref(),
                Err("camera file failed: unsupported format".to_owned()),
            );
        }

        feed.stop();
    });

    for track in captured.into_iter().flat_map(MediaStream::get_tracks) {
        track
            .unchecked_into::<MediaStreamTrack>()
            .set_onended(Some(ended.as_ref().unchecked_ref()));
    }

    match &player {
        Player::Video(video) if failed => video.set_onerror(Some(ended.as_ref().unchecked_ref())),
        Player::Image(image) => image.set_onerror(Some(ended.as_ref().unchecked_ref())),
        Player::Video(_) => {}
    }

    // The worker failed to load or panicked, so no frame comes back.
    let broke = Closure::<dyn Fn()>::new(move || {
        link.did(
            "camera",
            feed.device().as_ref(),
            Err("capture failed: worker".to_owned()),
        );
        feed.stop();
    });

    worker.set_onerror(Some(broke.as_ref().unchecked_ref()));

    // The caller feeds the camera from it (`Feed::feed_from`), which ends a
    // capture started alongside this one, so nothing is left running that
    // cannot be stopped. The camera stays held, because this capture's
    // channel feeds it (`media::Lanes` in tetherd).
    Ok(Running {
        lens,
        player,
        ticking,
        worker,
        _ended: ended,
        _broke: broke,
        _packed: packed,
    })
}
