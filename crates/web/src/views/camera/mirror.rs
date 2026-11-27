//! Nothing else on the phone can use a camera while it is shown here.

use std::any::Any;

use android::scrcpy::Facing;
use futures_channel::mpsc;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::{ev, html};
use protocol::PhoneCamera;

use super::mode::{Shown, key, size_name};
use super::style;
use super::turning::{MirrorTools, Turning};
use super::zoom::{ZoomBar, rounded, zoom_text};
use super::{Camera, Control, Mirrored};
use crate::app::Picked;
use crate::ui::kept::{KeptSignal, kept_signal};

const ZOOM_STEP: f32 = 1.0 + 1.0 / 16.0;
/// The wheel distance of one zoom step. A mouse wheel's notch is 100, so it
/// makes two steps.
const WHEEL_STEP: f64 = 50.0;

/// Each is None until the browser's database has answered.
#[derive(Clone, Copy)]
pub(super) struct Left {
    /// The turn in degrees while it is not locked.
    pub(super) turn: KeptSignal<u16>,
    /// True while the turn is kept for each orientation of the phone, and
    /// the picture follows the phone.
    pub(super) locked: KeptSignal<bool>,
    pub(super) by_portrait: KeptSignal<Option<u16>>,
    pub(super) by_landscape: KeptSignal<Option<u16>>,
    zoom: KeptSignal<f64>,
}

#[derive(Clone, Debug, PartialEq)]
enum Picture {
    Opening,
    Live,
    /// `drawn` if it had played, so that its last frame is still up.
    Stopped {
        why: String,
        drawn: bool,
    },
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Mirror(shown: Shown) -> impl IntoView {
    let device = expect_context::<Picked>().id.to_string();
    let camera = &shown.camera;
    // A camera's sensor lies on its side in the phone, usually a quarter
    // turn one way at the back and the other way at the front, which is
    // where its turn starts.
    let usual = match camera.facing {
        Facing::Back => 90,
        Facing::Front => 270,
        Facing::External | Facing::Unknown => 0,
    };
    let turn_key = key("turn", &device, camera);
    let left = Left {
        turn: kept_signal(turn_key.clone(), usual),
        locked: kept_signal(key("lock", &device, camera), true),
        by_portrait: kept_signal(format!("{turn_key}-portrait"), None),
        by_landscape: kept_signal(format!("{turn_key}-landscape"), None),
        zoom: kept_signal(key("zoom", &device, camera), 1.0),
    };
    // The camera opens at the zoom it was left at, so it waits for what was
    // kept. That is read once, and what changes later does not open it again.
    let read = Memo::new(move |_| {
        left.turn.get().is_some()
            && left.locked.get().is_some()
            && left.by_portrait.get().is_some()
            && left.by_landscape.get().is_some()
            && left.zoom.get().is_some()
    });
    let shown = StoredValue::new(shown);

    view! {
        <Show when=move || read.get()>
            <Opened shown=shown.get_value() left usual />
        </Show>
    }
}

#[component]
fn Opened(shown: Shown, left: Left, usual: u16) -> impl IntoView {
    let picked = expect_context::<Picked>();
    let listed = picked.listed;
    let mirrored = picked.device.get_untracked();
    let Shown { camera, mode } = shown;
    let video = NodeRef::<html::Video>::new();
    let picture = RwSignal::new(Picture::Opening);
    let live = move || picture.with(|picture| *picture == Picture::Live);
    let stopped = move |why: String| {
        picture.try_update(|picture| {
            let drawn = *picture == Picture::Live;

            *picture = Picture::Stopped { why, drawn };
        });
    };
    // Dropping it lets the camera go.
    let open = StoredValue::new_local(None::<(mpsc::UnboundedSender<Control>, Box<dyn Any>)>);
    let send = move |control: Control| {
        open.with_value(|open| {
            if let Some((sender, _)) = open {
                let _ = sender.unbounded_send(control);
            }
        });
    };
    let turning = Turning::new(left, usual, camera.facing);
    // A camera that gives no range does not zoom.
    let (least, most) = camera.zoom.unwrap_or((1.0, 1.0));
    let zoom = Signal::derive(move || (left.zoom.get().unwrap_or(1.0) as f32).clamp(least, most));
    let set_zoom = move |to: f32| {
        let to = rounded(to.clamp(least, most));

        send(Control::Zoom(to));
        left.zoom.set(f64::from(to));
    };

    // A trackpad pinch, which the browser reports as Ctrl and the wheel,
    // zooms a step at a time.
    let wheeled = StoredValue::new(0.0f64);
    let wheel = move |event: ev::WheelEvent| {
        if !event.ctrl_key() || !untrack(live) || least == most {
            return;
        }

        // Keeps the browser from zooming the page.
        event.prevent_default();

        let turned = wheeled.get_value() - event.delta_y();
        let by = (turned / WHEEL_STEP).trunc();

        wheeled.set_value(turned - by * WHEEL_STEP);

        if by != 0.0 {
            set_zoom(zoom.get_untracked() * ZOOM_STEP.powi(by as i32));
        }
    };

    // shortcut: scrcpy does not say which cameras have a flash, so a front
    // camera is taken to have none and any other to have one. Ask the
    // device's camera service if one turns out wrong.
    let has_torch = camera.facing != Facing::Front;
    let torch = RwSignal::new(false);
    let about = {
        let facing = camera.facing;
        let at = mode
            .as_ref()
            .map(|mode| format!(" · {} · {} fps", size_name(&mode.size), mode.rate));

        move || {
            let torch = if torch.get() { " · torch on" } else { "" };
            let name = listed.with(|listed| listed.name.clone());
            let at = at.as_deref().unwrap_or_default();

            format!(
                "The {facing} camera of {name}{at} · {}×{torch}",
                zoom_text(zoom.get())
            )
        }
    };

    spawn_local({
        let asked = PhoneCamera {
            id: camera.id.clone(),
            mode,
            zoom: zoom.get_untracked(),
        };

        async move {
            let Mirrored {
                stream,
                control,
                ended,
                kept,
            } = match Camera::mirror(&*mirrored, asked).await {
                Ok(opened) => opened,
                Err(error) => {
                    stopped(error);

                    return;
                }
            };

            if let Some(video) = video.get_untracked() {
                video.set_src_object(Some(&stream));
                // Playing fails when the stream is replaced at once, which is
                // fine. Awaited so that the rejection is handled.
                if let Ok(started) = video.play() {
                    spawn_local(async move {
                        let _ = wasm_bindgen_futures::JsFuture::from(started).await;
                    });
                }
            }

            // Closed meanwhile, which drops it and lets the camera go.
            if open.try_set_value(Some((control, kept))).is_some() {
                return;
            }

            stopped(ended.await);
        }
    });

    on_cleanup(move || {
        open.try_update_value(Option::take);
    });

    view! {
        <div class=style::stage on:wheel=wheel>
            // A quarter turn lays the video on its side, so it is sized the
            // other way round to fill the stage once turned.
            <video
                node_ref=video
                class=style::mirrored
                class=(style::sideways, move || turning.shown() % 180 == 90)
                style=move || format!("transform: rotate({}deg)", turning.shown())
                hidden=move || {
                    picture.with(|picture| {
                        matches!(picture, Picture::Opening | Picture::Stopped { drawn: false, .. })
                    })
                }
                muted
                autoplay
                playsinline
                on:playing=move |_| {
                    picture.update(|picture| {
                        if *picture == Picture::Opening {
                            *picture = Picture::Live;
                        }
                    });
                }
            ></video>
            {move || {
                let said = match picture.get() {
                    Picture::Opening => "Opening the camera…".to_owned(),
                    Picture::Stopped { why, .. } => why,
                    Picture::Live => return None,
                };

                Some(view! { <span class=style::saying role="status">{said}</span> })
            }}
            <MirrorTools
                turning
                torch
                has_torch
                live=Signal::derive(live)
                on_torch=move |on: bool| send(Control::Torch(on))
            />
        </div>
        <Show when=move || least < most>
            <ZoomBar least most zoom on_zoom=move |to: f32| set_zoom(to) />
        </Show>
        <div class=style::under>
            <span>{about}</span>
        </div>
    }
}
