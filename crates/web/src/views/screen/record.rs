use std::time::Duration;

use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{
    Blob, BlobEvent, CanvasRenderingContext2d, HtmlAnchorElement, HtmlCanvasElement,
    HtmlVideoElement, MediaRecorder, MediaStream, Url,
};

use protocol::DeviceId;

use crate::app::Global;
use crate::link::Link;

struct Running {
    recorder: MediaRecorder,
    parts: js_sys::Array,
    seconds: u32,
    ticking: Option<IntervalHandle>,
    /// Called by the recorder with each chunk, so it lives as long as the
    /// recording.
    _part: Closure<dyn Fn(BlobEvent)>,
}

#[derive(Clone, Copy)]
pub(super) struct Recording {
    running: RwSignal<Option<Running>, LocalStorage>,
    /// A memo, since `running` changes with every second it counts.
    on: Memo<bool>,
    link: Link,
    id: StoredValue<DeviceId>,
}

impl Recording {
    pub(super) fn new(id: &DeviceId) -> Self {
        let running = RwSignal::new_local(None::<Running>);

        Recording {
            running,
            on: Memo::new(move |_| running.with(Option::is_some)),
            link: expect_context::<Global>().link,
            id: StoredValue::new(id.clone()),
        }
    }

    pub(super) fn on(self) -> bool {
        self.on.get()
    }

    pub(super) fn seconds(self) -> Option<u32> {
        self.running
            .with(|running| running.as_ref().map(|running| running.seconds))
    }

    pub(super) fn toggle(self, video: NodeRef<html::Video>) {
        let link = self.link;
        let id = self.id;
        let running = self.running;
        // The recorder calls `done` below once it has stopped.
        let stopping = running.with_untracked(|running| match running {
            Some(running) => {
                let _ = running.recorder.stop();

                true
            }
            None => false,
        });

        if stopping {
            return;
        }

        let started = video
            .get_untracked()
            .and_then(|video| MediaRecorder::new_with_media_stream(&captured(&video)?).ok());

        let Some(recorder) = started else {
            link.say("screen recording unsupported in this browser");
            return;
        };

        let parts = js_sys::Array::new();
        let taken = parts.clone();
        let part = Closure::<dyn Fn(BlobEvent)>::new(move |event: BlobEvent| {
            if let Some(part) = event.data() {
                taken.push(&part);
            }
        });

        // Called once and freed by that call. It drops the `Running` that
        // holds the chunk handler, so it cannot be stored in `Running`.
        let done = Closure::once_into_js(move |_: web_sys::Event| {
            let Some(ended) = running.try_update(Option::take).flatten() else {
                return;
            };

            if let Some(ticking) = ended.ticking {
                ticking.clear();
            }

            let whole = Blob::new_with_blob_sequence(&ended.parts);

            match whole.and_then(|whole| Url::create_object_url_with_blob(&whole)) {
                Ok(address) => {
                    save(&address, "webm");
                    let _ = Url::revoke_object_url(&address);
                }
                Err(_) => link.did(
                    "screen",
                    id.try_get_value().as_ref(),
                    Err("recording failed: not saved".to_owned()),
                ),
            }
        });

        recorder.set_ondataavailable(Some(part.as_ref().unchecked_ref()));
        recorder.set_onstop(Some(done.unchecked_ref()));

        if recorder.start().is_err() {
            return;
        }

        let second = move || {
            running.try_update(|running| {
                if let Some(running) = running {
                    running.seconds += 1;
                }
            });
        };

        running.set(Some(Running {
            recorder,
            parts,
            seconds: 0,
            ticking: set_interval_with_handle(second, Duration::from_secs(1)).ok(),
            _part: part,
        }));
    }
}

/// The stream a video element plays, through its `captureStream`, which
/// web-sys does not have. It is silent while the element is muted.
fn captured(video: &HtmlVideoElement) -> Option<MediaStream> {
    let capture = js_sys::Reflect::get(video, &"captureStream".into()).ok()?;
    let capture: js_sys::Function = capture.dyn_into().ok()?;

    capture.call0(video).ok()?.dyn_into().ok()
}

/// The canvas only holds tetherd's pictures of a booting or stopping
/// emulator, which a screenshot captures while no video shows.
pub(super) fn screenshot(video: NodeRef<html::Video>, canvas: NodeRef<html::Canvas>) {
    let shown = video
        .get_untracked()
        .filter(|video| video.src_object().is_some() && video.video_width() > 0);
    let image = match shown {
        Some(video) => frame_of(&video),
        None => canvas
            .get_untracked()
            .and_then(|canvas| canvas.to_data_url().ok()),
    };

    if let Some(image) = image {
        save(&image, "png");
    }
}

fn frame_of(video: &HtmlVideoElement) -> Option<String> {
    let canvas: HtmlCanvasElement = document().create_element("canvas").ok()?.unchecked_into();

    canvas.set_width(video.video_width());
    canvas.set_height(video.video_height());

    let context: CanvasRenderingContext2d = canvas.get_context("2d").ok()??.unchecked_into();

    context
        .draw_image_with_html_video_element(video, 0.0, 0.0)
        .ok()?;
    canvas.to_data_url().ok()
}

fn save(address: &str, kind: &str) {
    if let Ok(anchor) = document().create_element("a") {
        let anchor: HtmlAnchorElement = anchor.unchecked_into();
        let now = String::from(js_sys::Date::new_0().to_iso_string()).replace(':', "-");

        anchor.set_href(address);
        anchor.set_download(&format!("screen-{now}.{kind}"));
        anchor.click();
    }
}
