//! The stage only draws the framing. link/capture.rs applies it to the
//! frames, and tetherd to a feed from its machine.

use std::time::Duration;

use leptos::prelude::*;
use leptos::{ev, html};
use protocol::Framing;
use stylance::classes;
use wasm_bindgen::JsCast;

use super::fed::Cam;
use super::geometry;
use super::source::Source;
use super::style;
use crate::app::{Icon, Picked, icons};
use crate::ui::button;
use crate::ui::turn_tools::TurnTools;

/// The longest the frame waits for a new source to say its shape.
const SETTLES_IN: Duration = Duration::from_secs(1);

/// From 0 to 1 each way across `pad`.
fn place(pad: NodeRef<html::Div>, event: &ev::PointerEvent) -> Option<(f32, f32)> {
    let place = pad.get_untracked()?.get_bounding_client_rect();

    Some((
        ((event.client_x() - place.left()) / place.width()) as f32,
        ((event.client_y() - place.top()) / place.height()) as f32,
    ))
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Moving,
    Sizing,
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Stage(
    cam: Cam,
    #[prop(into)] upright: Signal<bool>,
    shown: Memo<Source>,
    #[prop(into)] blank: Signal<String>,
) -> impl IntoView {
    let feed = expect_context::<Picked>().feed;
    let live = move || shown.get() != Source::Off;
    // Everything that tells one source from the next, since another camera,
    // screen, webcam or file is another picture with a shape of its own.
    let playing = Memo::new(move |_| {
        live().then(|| {
            (
                feed.capturing(),
                feed.remoting(),
                feed.played().map(|file| file.url),
            )
        })
    });
    let framing = move || feed.framing();
    let frame = move |next: Framing| feed.frame(geometry::clamped(next));
    let camera = Signal::derive(move || {
        protocol::camera_shape((cam.settings.width, cam.settings.height), upright.get())
    });

    view! {
        <div class=style::stage>
            {move || match playing.get() {
                // Built again for each source, so each starts without the
                // shape of the one before.
                Some(_) => view! { <Pad camera /> }.into_any(),
                None => view! {
                    <div class=classes!(style::pad, style::blank)>
                        <span>{move || blank.get()}</span>
                    </div>
                }
                .into_any(),
            }}
            <div class=style::tools>
                <TurnTools
                    turn=Signal::derive(move || u16::from(framing().quarters) * 90)
                    // Turning the source moves the frame back to the middle.
                    on_turn=move |degrees: u16| {
                        frame(Framing {
                            quarters: (degrees / 90 % 4) as u8,
                            x: 0.5,
                            y: 0.5,
                            ..untrack(framing)
                        });
                    }
                    disabled=Signal::derive(move || !live())
                />
                <span class=style::rule aria-hidden="true"></span>
                <button
                    type="button"
                    class=button::style::icon
                    class=(button::style::active, move || live() && framing().mirrored)
                    title="Mirror"
                    aria-label="Mirror the picture"
                    aria-pressed=move || framing().mirrored.to_string()
                    disabled=move || !live()
                    on:click=move |_| {
                        let now = untrack(framing);

                        frame(Framing {
                            mirrored: !now.mirrored,
                            x: 1.0 - now.x,
                            ..now
                        });
                    }
                >
                    <Icon paths=icons::MIRROR />
                </button>
                <span class=style::rule aria-hidden="true"></span>
                <button
                    type="button"
                    class=button::style::icon
                    title="Reset the frame"
                    aria-label="Put the frame back where it started"
                    disabled=move || !live()
                    on:click=move |_| frame(Framing::default())
                >
                    <Icon paths=icons::FRAME />
                </button>
            </div>
        </div>
    }
}

#[component]
fn Pad(
    /// Width over height.
    camera: Signal<f32>,
) -> impl IntoView {
    let feed = expect_context::<Picked>().feed;
    // This is built again for another source, so the file is read once.
    let file = untrack(|| feed.played());
    let framing = move || feed.framing();
    let frame = move |next: Framing| feed.frame(geometry::clamped(next));
    let pad = NodeRef::<html::Div>::new();
    let played = NodeRef::<html::Video>::new();
    let still = NodeRef::<html::Img>::new();
    let drag = StoredValue::new(None::<Drag>);
    // Width and height before the source is turned. None until it has said.
    let local_shape = RwSignal::new(None::<(u32, u32)>);
    // The frame hides until the source's shape is known, so it is not drawn
    // in the camera's first. A source that never says is waited on for
    // `SETTLES_IN`.
    let settling = RwSignal::new(true);

    set_timeout(
        move || {
            settling.try_set(false);
        },
        SETTLES_IN,
    );

    let measure = move || {
        if let Some(video) = played.get_untracked()
            && video.video_width() > 0
            && video.video_height() > 0
        {
            local_shape.set(Some((video.video_width(), video.video_height())));
        }

        if let Some(image) = still.get_untracked()
            && image.natural_width() > 0
            && image.natural_height() > 0
        {
            local_shape.set(Some((image.natural_width(), image.natural_height())));
        }
    };

    // A source the browser captures or tetherd previews plays from its
    // stream. A file plays from its address.
    Effect::new(move |_| {
        if let (Some(video), Some(stream)) = (played.get(), feed.stream()) {
            // It plays by itself through autoplay. Calling play() would reject
            // if the element left the page before it started.
            video.set_src_object(Some(&stream));
        }
    });

    let given = move || {
        local_shape.get().map_or_else(
            || camera.get(),
            |shape| geometry::given_shape(shape, framing().quarters),
        )
    };
    let turned = move || geometry::turned_style(framing(), given());
    let aim = move |event: &ev::PointerEvent| {
        if let Some((x, y)) = place(pad, event) {
            frame(Framing {
                x,
                y,
                ..untrack(framing)
            });
        }
    };
    let size = move |event: &ev::PointerEvent| {
        if let Some((x, _)) = place(pad, event) {
            frame(geometry::resized(
                untrack(framing),
                untrack(given),
                camera.get_untracked(),
                x,
            ));
        }
    };
    let picture = match file {
        Some(file) if file.image => {
            view! { <img node_ref=still src=file.url alt="" style=turned on:load=move |_| measure() /> }
                .into_any()
        }
        Some(file) => view! {
            <video
                node_ref=played
                src=file.url
                autoplay
                // The property, since the attribute does not mute an
                // element made from script.
                prop:muted=true
                loop
                playsinline
                style=turned
                on:loadedmetadata=move |_| measure()
            ></video>
        }
        .into_any(),
        None => view! {
            <video
                node_ref=played
                autoplay
                prop:muted=true
                playsinline
                style=turned
                on:loadedmetadata=move |_| measure()
                on:resize=move |_| measure()
            ></video>
        }
        .into_any(),
    };

    view! {
        <div
            node_ref=pad
            class=style::pad
            style=move || {
                let given = given();

                format!("aspect-ratio: {given}; width: min(100cqw, calc(100cqh * {given}))")
            }
            title="Drag to move it. Drag its corner to size it."
            on:pointerdown=move |event| {
                if let Some(pad) = pad.get_untracked() {
                    let _ = pad.set_pointer_capture(event.pointer_id());
                }

                drag.set_value(Some(Drag::Moving));
                aim(&event);
            }
            on:pointermove=move |event| {
                if drag.get_value() == Some(Drag::Moving) {
                    aim(&event);
                }
            }
            on:pointerup=move |_| drag.set_value(None)
            on:pointercancel=move |_| drag.set_value(None)
            on:wheel=move |event| {
                event.prevent_default();

                let now = untrack(framing);

                frame(Framing {
                    zoom: now.zoom * (1.0015f32).powf(-event.delta_y() as f32),
                    ..now
                });
            }
        >
            {picture}
            <div
                class=style::seen
                style=move || {
                    let (left, top, width, height) = framing().seen(given(), camera.get());
                    let hidden = if settling.get() && local_shape.with(Option::is_none) {
                        "; visibility: hidden"
                    } else {
                        ""
                    };

                    format!(
                        "left: {}%; top: {}%; width: {}%; height: {}%{hidden}",
                        left * 100.0,
                        top * 100.0,
                        width * 100.0,
                        height * 100.0,
                    )
                }
            >
                <span>"What the device sees"</span>
                <button
                    type="button"
                    aria-label="Size of what the device sees"
                    on:pointerdown=move |event| {
                        // The pad under it would take the press as a move.
                        event.stop_propagation();

                        if let Some(corner) = event
                            .current_target()
                            .and_then(|corner| corner.dyn_into::<web_sys::Element>().ok())
                        {
                            let _ = corner.set_pointer_capture(event.pointer_id());
                        }

                        drag.set_value(Some(Drag::Sizing));
                    }
                    on:pointermove=move |event| {
                        if drag.get_value() == Some(Drag::Sizing) {
                            size(&event);
                        }
                    }
                    on:pointerup=move |_| drag.set_value(None)
                    on:pointercancel=move |_| drag.set_value(None)
                ></button>
            </div>
        </div>
    }
}
