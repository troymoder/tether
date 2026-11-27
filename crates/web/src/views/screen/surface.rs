use leptos::prelude::*;
use leptos::{ev, html};

use super::{Button, Input, TouchAction, style};
use crate::app::Picked;

/// Added to a finger's pointer number for the mirrored finger of a pinch.
const MIRRORED: u32 = 1 << 30;

#[derive(Clone, PartialEq)]
pub(super) struct Finger {
    pub(super) pointer: u32,
    pub(super) x: f64,
    pub(super) y: f64,
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Surface(
    canvas: NodeRef<html::Canvas>,
    /// The pane draws the fingers, since a mark placed in window coordinates
    /// cannot sit inside the screen's container.
    fingers: RwSignal<Vec<Finger>>,
    #[prop(into)] shape: Signal<String>,
    #[prop(into)] drawn: Signal<bool>,
    #[prop(into)] has_back: Signal<bool>,
) -> impl IntoView {
    let control = expect_context::<Picked>().control;
    let pinching = StoredValue::new(false);

    // Maps a page point to a point on the video, with the video's size,
    // which a device may check against the size it sends.
    let place = move |x: f64, y: f64| {
        let canvas = canvas.get_untracked()?;
        let bounds = canvas.get_bounding_client_rect();
        let (width, height) = control.size()?;
        let width = f64::from(width);
        let height = f64::from(height);
        let across = ((x - bounds.left()) / bounds.width()).clamp(0.0, 1.0);
        let down = ((y - bounds.top()) / bounds.height()).clamp(0.0, 1.0);

        Some((
            (across * width) as f32,
            (down * height) as f32,
            width as u16,
            height as u16,
        ))
    };

    let finger = move |action: TouchAction, pointer: u32, at: (f64, f64)| {
        let placed = place(at.0, at.1);

        // A finger whose video stopped under it is removed, so its mark does
        // not linger.
        fingers.update(|fingers| {
            fingers.retain(|finger| finger.pointer != pointer);

            if placed.is_some() && matches!(action, TouchAction::Down | TouchAction::Move) {
                fingers.push(Finger {
                    pointer,
                    x: at.0,
                    y: at.1,
                });
            }
        });

        let Some((x, y, width, height)) = placed else {
            return;
        };

        control.send(Input::Touch {
            action,
            pointer,
            x,
            y,
            width,
            height,
        });
    };

    let touch = move |action: TouchAction, event: &ev::PointerEvent| {
        event.prevent_default();

        // Every pointer is a finger, including the mouse.
        let pointer = event.pointer_id() as u32;
        let at = (event.client_x(), event.client_y());

        finger(action, pointer, at);

        // With Alt held as the press starts, a second finger mirrors the
        // first through the middle of the screen, which makes a pinch.
        if pinching.get_value()
            && let Some(canvas) = canvas.get_untracked()
        {
            let bounds = canvas.get_bounding_client_rect();
            let mirrored = (
                bounds.left() + bounds.right() - at.0,
                bounds.top() + bounds.bottom() - at.1,
            );

            finger(action, pointer | MIRRORED, mirrored);
        }
    };

    let held = StoredValue::new(Vec::<String>::new());
    let key = move |down: bool, event: ev::KeyboardEvent| {
        // Leaves the browser's own shortcuts alone, and paste, which `paste`
        // handles.
        if event.meta_key() || (event.ctrl_key() && event.code() == "KeyV") {
            return;
        }

        event.prevent_default();

        let code = event.code();

        held.update_value(|held| {
            held.retain(|held| *held != code);

            if down {
                held.push(code.clone());
            }
        });

        control.send(Input::Key {
            down,
            code,
            shift: event.shift_key(),
            ctrl: event.ctrl_key(),
            alt: event.alt_key(),
        });
    };

    // Releases the keys that are down when focus leaves, since their key ups
    // go elsewhere then (Alt+Tab, for one).
    let release = move |_| {
        for code in held.try_update_value(std::mem::take).unwrap_or_default() {
            control.send(Input::Key {
                down: false,
                code,
                shift: false,
                ctrl: false,
                alt: false,
            });
        }
    };

    let paste = move |event: web_sys::ClipboardEvent| {
        let text = event
            .clipboard_data()
            .and_then(|data| data.get_data("text").ok())
            .unwrap_or_default();

        if !text.is_empty() {
            control.send(Input::Text(text));
        }
    };

    view! {
        <canvas
            node_ref=canvas
            class=style::surface
            class=(style::hidden, move || !drawn.get())
            // Focusable so it can take keyboard input.
            tabindex="0"
            on:keydown=move |event| key(true, event)
            on:keyup=move |event| key(false, event)
            on:blur=release
            on:paste=paste
            style=move || shape.get()
            on:pointerdown=move |event| {
                if event.button() != 0 {
                    return;
                }

                // Captures the pointer so a drag that leaves the screen still
                // ends.
                if let Some(canvas) = canvas.get_untracked() {
                    let _ = canvas.set_pointer_capture(event.pointer_id());
                    let _ = canvas.focus();
                }

                pinching.set_value(event.alt_key());
                touch(TouchAction::Down, &event);
            }
            on:pointermove=move |event| {
                if event.buttons() & 1 != 0 {
                    touch(TouchAction::Move, &event);
                }
            }
            on:pointerup=move |event| {
                if event.button() == 0 {
                    touch(TouchAction::Up, &event);
                    pinching.set_value(false);
                }
            }
            on:pointercancel=move |event| {
                touch(TouchAction::Cancel, &event);
                pinching.set_value(false);
            }
            on:contextmenu=move |event| {
                event.prevent_default();

                if has_back.get_untracked() {
                    control.send(Input::Button(Button::Back));
                }
            }
            on:wheel=move |event| {
                let at = (event.client_x(), event.client_y());

                if let Some((x, y, width, height)) = place(at.0, at.1) {
                    control.send(Input::Scroll {
                        x,
                        y,
                        width,
                        height,
                        dx: (-event.delta_x() / 100.0) as f32,
                        dy: (-event.delta_y() / 100.0) as f32,
                    });
                }
            }
        ></canvas>
    }
}
