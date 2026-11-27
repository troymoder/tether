//! The video plays in a `<video>` over a canvas, which takes the input and
//! shows tetherd's pictures of an emulator that is booting or stopping.
//!
//! The pane shows one state, `Control::playing` (screen/playing.rs). The
//! stream (screen/stream.rs) advances it and the parts here read it.

use leptos::html;
use leptos::prelude::*;

use crate::app::Picked;
use crate::ui::kept::kept_signal;

mod absent;
mod control;
mod controls;
mod cover;
mod device;
mod device_button;
mod geometry;
mod keys;
mod playing;
mod record;
mod stats;
mod stream;
mod surface;
mod waiting;

pub(crate) use absent::Absent;
pub(crate) use control::Control;
pub(crate) use device::{Button, Features, Input, Screen, Session, Told, TouchAction, Turned};

use controls::{ControlsBar, FULL};
use cover::Cover;
use keys::{NavKeys, SideKeys};
use playing::Size;
use record::Recording;
use stream::Stream;
use surface::{Finger, Surface};

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "screen.module.scss"
);

const VOLUME_KEY: &str = "device-volume";

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Screen() -> impl IntoView {
    let Picked {
        id,
        device,
        listed,
        control,
        ..
    } = expect_context::<Picked>();
    let canvas = NodeRef::<html::Canvas>::new();
    let video = NodeRef::<html::Video>::new();
    let stream = Stream::new(canvas, video);
    let recording = Recording::new(&id);
    let fingers = RwSignal::new(Vec::<Finger>::new());
    // Sound starts off because browsers only play sound the user asked for.
    let sound_on = RwSignal::new(false);
    // In percent. Sound starts off, so the kept volume is loaded before
    // anything can be heard.
    let volume = kept_signal(VOLUME_KEY, FULL);
    let features = Memo::new(move |_| Screen::features(&*device.get()));
    let resting = Memo::new(move |_| listed.with(waiting::resting));
    let drawn = Memo::new(move |_| control.playing.with(|playing| playing.drawn()));
    // The pane keeps the shape of the last picture after it is gone, so the
    // device does not change shape as it stops or disconnects.
    let size = Memo::new(move |last: Option<&Size>| {
        drawn.get().or(last.copied()).unwrap_or(geometry::SHAPE)
    });
    let shape = Signal::derive(move || {
        let (width, height) = size.get();

        format!("--width: {width}; --height: {height}")
    });

    stream.follow();
    on_cleanup(move || stream.stop());

    view! {
        // app.module.scss and app/drop.rs find the device's pane by this id.
        <aside id="phone" class=style::phone aria-label="Device">
            <ControlsBar video canvas stream recording features sound_on volume />
            <div class=style::screen>
                <div class=style::handset style=move || shape.get()>
                    <Show when=move || recording.on()>
                        <div class=style::recording_mark role="status">
                            {move || {
                                recording
                                    .seconds()
                                    .map(|seconds| {
                                        format!("● {}:{:02}", seconds / 60, seconds % 60)
                                    })
                            }}
                        </div>
                    </Show>
                    <div class=style::face>
                        <video
                            node_ref=video
                            class=style::video
                            prop:muted=move || !sound_on.get()
                            prop:volume=move || f64::from(volume.get().unwrap_or(FULL)) / 100.0
                            autoplay=true
                            playsinline=true
                        ></video>
                        <Surface
                            canvas
                            fingers
                            shape
                            drawn=Signal::derive(move || drawn.get().is_some())
                            has_back=Signal::derive(move || features.get().contains(Features::BACK))
                        />
                        <Cover
                            resting
                            stream
                            shape
                            drawn=Signal::derive(move || drawn.get().is_some())
                        />
                    </div>
                    <NavKeys features />
                    <SideKeys features resting />
                </div>
            </div>
            {move || {
                fingers
                    .get()
                    .into_iter()
                    .map(|Finger { x, y, .. }| {
                        view! {
                            <div class=style::finger style=format!("left: {x}px; top: {y}px")></div>
                        }
                    })
                    .collect_view()
            }}
        </aside>
    }
}
