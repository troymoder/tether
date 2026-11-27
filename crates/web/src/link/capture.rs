//! At the camera's frame rate a frame of the source is handed to a worker
//! (running.rs and link/worker.rs). A frame is only handed over once the
//! previous one came back, so a slow machine sends fewer.
// shortcut: a browser throttles a background page to about one frame a
// second, and the camera with it. Take the frames in the worker too if that matters.
//
// One browser feeds a camera at a time. tetherd drops another browser's
// frames until it takes the camera over, and this one then stops (`heed`).

use js_sys::{Object, Reflect};
use leptos::prelude::*;
use protocol::{DeviceId, Feeder, Framing, Lens, Topic, encode};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{MediaStream, MediaStreamTrack};

use super::Link;

mod browser;
mod remote;
mod running;

pub(crate) use browser::{Capture, Played, SIZES, Wanted, cameras};
pub(crate) use remote::{Remote, Remoting, emulator_options, find, look, webcams};
pub(crate) use running::Sending;

use running::Running;

/// Made when the device is picked, because a feed goes on while other views
/// are open.
#[derive(Clone, Copy)]
pub(crate) struct Feed {
    link: Link,
    // Derived from the link's listing of the device. An emulator's id is its
    // console port, so another AVD can come up under the same id.
    avd: Memo<Option<String>>,
    /// The browser that took the camera over. Kept separate from `feeding`,
    /// which is idle by then.
    pub lost_camera: RwSignal<Option<Feeder>>,
    feeding: RwSignal<Feeding, LocalStorage>,
    /// Kept separate from `feeding`, so the reading each second does not
    /// rerun the readers of the source.
    sending: RwSignal<Option<Sending>>,
    framing: RwSignal<Framing>,
    lossless: RwSignal<bool>,
    /// Counts the times capturing was started or stopped. The browser takes a
    /// moment to grant a camera, and a capture that was stopped or restarted
    /// meanwhile is dropped when it arrives.
    turns: StoredValue<u32>,
}

enum Feeding {
    Idle,
    Capturing {
        wanted: Wanted,
        stream: MediaStream,
        running: Running,
    },

    Playing {
        played: Played,
        running: Running,
    },

    Remote {
        remoting: Remoting,
        preview: remote::Preview,
    },
}

impl Feeding {
    fn running(&self) -> Option<&Running> {
        match self {
            Feeding::Capturing { running, .. } | Feeding::Playing { running, .. } => Some(running),
            Feeding::Idle | Feeding::Remote { .. } => None,
        }
    }

    fn end(self) {
        match self {
            Feeding::Idle => {}
            Feeding::Capturing {
                stream, running, ..
            } => {
                running.end();
                browser::release_stream(&stream);
            }
            Feeding::Playing { played, running } => {
                running.end();
                browser::release_file(&played.url);
            }
            // Dropping the preview closes its lane and stops its drawing.
            Feeding::Remote { .. } => {}
        }
    }
}

impl Feed {
    /// This browser's own sending stops when the picked device's owner is
    /// cleaned up. A webcam or file on tetherd's machine keeps feeding.
    pub(crate) fn start(link: Link, id: &DeviceId) -> Self {
        let id = id.clone();
        let avd = Memo::new(move |_| link.run(&id).map(|run| run.avd));
        let feed = Feed {
            link,
            avd,
            lost_camera: RwSignal::new(None),
            feeding: RwSignal::new_local(Feeding::Idle),
            sending: RwSignal::new(None),
            framing: RwSignal::new(Framing::default()),
            lossless: RwSignal::new(false),
            turns: StoredValue::new(0),
        };
        // The settings of the emulator fed from here give its camera's shape,
        // which is needed while it is fed with the Camera view closed.
        let fed = Memo::new(move |_| feed.feeding.with(|feeding| feeding.running().is_some()));

        link.follow(move || {
            fed.get()
                .then(|| feed.avd.get())
                .flatten()
                .map(Topic::Emulator)
        });
        on_cleanup(move || feed.stop());

        feed
    }

    pub(crate) fn device(self) -> Option<DeviceId> {
        let avd = self.avd.try_get_untracked().flatten()?;

        untrack(|| self.link.emulator(&avd)).map(|device| device.id)
    }

    fn emulator(self) -> Result<String, String> {
        self.avd
            .try_get_untracked()
            .flatten()
            .ok_or_else(|| "camera failed: not an emulator".to_owned())
    }

    fn feed_from(self, feeding: Feeding) {
        let mut next = Some(feeding);
        let before = self
            .feeding
            .try_update(|now| next.take().map(|next| std::mem::replace(now, next)));

        // `next` is still set if the feed is gone with its device. It is
        // ended too, because nothing else would stop it.
        for over in before.flatten().into_iter().chain(next) {
            over.end();
        }
    }

    pub(crate) fn capturing(self) -> Option<Wanted> {
        self.feeding.with(|feeding| match feeding {
            Feeding::Capturing { wanted, .. } => Some(wanted.clone()),
            _ => None,
        })
    }

    pub(crate) fn sent(self) -> Option<&'static str> {
        self.feeding.with(|feeding| match feeding {
            Feeding::Capturing { wanted, .. } if wanted.what == Capture::Screen => {
                Some("Sharing this screen with the device")
            }
            Feeding::Capturing { .. } => Some("Sending this camera to the device"),
            Feeding::Playing { .. } => Some("Sending a file to the device"),
            Feeding::Idle | Feeding::Remote { .. } => None,
        })
    }

    pub(crate) fn sending(self) -> Option<Sending> {
        self.sending.get()
    }

    pub(crate) fn label(self) -> Option<String> {
        let stream = self.stream()?;
        let track: MediaStreamTrack = stream.get_video_tracks().get(0).dyn_into().ok()?;

        Some(track.label()).filter(|label| !label.is_empty())
    }

    pub(crate) fn lossless(self) -> bool {
        self.lossless.get()
    }

    pub(crate) fn set_lossless(self, on: bool) {
        running::set_lossless(self, on);
    }

    fn turn(self) -> Option<u32> {
        self.turns.try_update_value(|turns| {
            *turns = turns.wrapping_add(1);
            *turns
        })
    }

    pub(crate) fn framing(self) -> Framing {
        self.framing.get()
    }

    /// A webcam on tetherd's machine is framed there, so tetherd gets the new
    /// framing.
    pub(crate) fn frame(self, framing: Framing) {
        self.framing.set(framing);
        self.feeding.try_with_untracked(|feeding| {
            if let Feeding::Remote { preview, .. } = feeding {
                preview.lane.send(&encode(&framing));
            }
        });
    }

    pub(crate) fn played(self) -> Option<Played> {
        self.feeding.with(|feeding| match feeding {
            Feeding::Playing { played, .. } => Some(played.clone()),
            _ => None,
        })
    }

    pub(crate) fn remoting(self) -> Option<Remoting> {
        self.feeding.with(|feeding| match feeding {
            Feeding::Remote { remoting, .. } => Some(remoting.clone()),
            _ => None,
        })
    }

    pub(crate) fn stream(self) -> Option<MediaStream> {
        // `Clone::clone` is another handle to the same stream. The browser's
        // `MediaStream.clone` would make a stream with its own tracks on the
        // same camera, which are never stopped and keep the camera on.
        self.feeding.with(|feeding| match feeding {
            Feeding::Capturing { stream, .. } => Some(<MediaStream as Clone>::clone(stream)),
            Feeding::Remote { preview, .. } => Some(<MediaStream as Clone>::clone(&preview.stream)),
            Feeding::Idle | Feeding::Playing { .. } => None,
        })
    }

    pub(crate) fn capture(self, wanted: Wanted) {
        leptos::task::spawn_local(async move {
            if let Err(error) = browser::start(self, wanted).await {
                self.link.did("camera", self.device().as_ref(), Err(error));
            }
        });
    }

    pub(crate) fn show(self, lens: Lens, file: &web_sys::File) -> Result<(), String> {
        browser::show(self, lens, file)
    }

    pub(crate) fn start_remote(self, wanted: Remoting) {
        if let Err(error) = remote::start_remote(self, wanted) {
            self.link.did("camera", self.device().as_ref(), Err(error));
        }
    }

    pub(crate) fn watch_remote(self, wanted: Remoting) {
        remote::watch_remote(self, wanted);
    }

    pub(crate) fn stop_remote(self) {
        remote::stop_remote(self);
    }

    /// A source on tetherd's machine keeps running until `stop_remote`, and
    /// only its preview here stops.
    pub(crate) fn stop(self) {
        self.turn();

        // Readers are only notified if something was fed.
        let before = self.feeding.try_maybe_update(|feeding| {
            let before = std::mem::replace(feeding, Feeding::Idle);

            (!matches!(before, Feeding::Idle), before)
        });

        if let Some(before) = before {
            before.end();
        }

        if self.sending.try_with_untracked(Option::is_some) == Some(true) {
            self.sending.set(None);
        }
    }
}

pub(crate) fn object(properties: &[(&str, JsValue)]) -> Object {
    let object = Object::new();

    for (key, value) in properties {
        let _ = Reflect::set(&object, &(*key).into(), value);
    }

    object
}
