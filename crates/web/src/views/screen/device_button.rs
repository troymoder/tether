use std::time::Duration;

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::{Button, Input, style};
use crate::app::{Icon, Picked};
use crate::ui::button;

/// Ticks of REPEATS_EVERY a volume button is held before it repeats. Ten
/// ticks of 50 ms match Android's own key repeat delay of 500 ms.
const REPEATS_AFTER: u32 = 10;
const REPEATS_EVERY: Duration = Duration::from_millis(50);

#[derive(Clone, Copy)]
enum Hold {
    Up,
    Pressed(Option<IntervalHandle>),
    /// Held down by a right-click, until it is pressed again.
    LeftDown(Option<IntervalHandle>),
}

/// A device button that stays down for as long as it is held in the page,
/// so the device sees a held button (power menu, continuous volume,
/// assistant). A right-click keeps it down until pressed again, for
/// combinations of two buttons.
#[allow(unreachable_pub)]
#[component]
pub(super) fn DeviceButton(
    button: Button,
    title: &'static str,
    paths: &'static str,
) -> impl IntoView {
    let control = expect_context::<Picked>().control;
    let hold = move |down: bool, repeat: u32| {
        control.send(Input::Hold {
            button,
            down,
            repeat,
        });
    };

    let held = RwSignal::new(Hold::Up);
    let left_down = move || matches!(held.get(), Hold::LeftDown(_));
    // A held volume button is resent with a repeat count, as a real one
    // repeats, and a device with no use for repeats skips them. Other buttons
    // are sent once, since a resend may count as a new press, and two power
    // presses can open the camera.
    let repeats = matches!(button, Button::VolumeUp | Button::VolumeDown);
    let press = move || {
        hold(true, 0);

        let ticks = StoredValue::new(0u32);
        let again = move || {
            let ticks = ticks.try_update_value(|ticks| {
                *ticks += 1;
                *ticks
            });

            if let Some(repeat) = ticks.and_then(|ticks| ticks.checked_sub(REPEATS_AFTER))
                && repeat > 0
            {
                hold(true, repeat);
            }
        };

        repeats
            .then(|| set_interval_with_handle(again, REPEATS_EVERY).ok())
            .flatten()
    };

    let let_go = move || {
        let Some(Hold::Pressed(repeating) | Hold::LeftDown(repeating)) =
            held.try_update(|held| std::mem::replace(held, Hold::Up))
        else {
            return;
        };

        if let Some(repeating) = repeating {
            repeating.clear();
        }

        hold(false, 0);
    };

    on_cleanup(let_go);

    view! {
        <button
            class=button::style::icon
            class=(style::held, left_down)
            title=format!("{title}. Right-click holds it down.")
            aria-label=move || if left_down() { format!("{title}, held down") } else { title.to_owned() }
            aria-pressed=move || left_down().to_string()
            disabled=move || !control.live()
            on:pointerdown=move |event| {
                if event.button() != 0 || !matches!(held.get_untracked(), Hold::Up) {
                    return;
                }

                // Captures the pointer so the button is released wherever the
                // pointer goes up.
                if let Some(pressed) = event.target() {
                    let pressed: web_sys::Element = pressed.unchecked_into();
                    let _ = pressed.set_pointer_capture(event.pointer_id());
                }

                held.set(Hold::Pressed(press()));
            }
            // A button held by a right-click is released by a left press here
            // or another right-click.
            on:pointerup=move |event| {
                if event.button() == 0 {
                    let_go();
                }
            }
            on:pointercancel=move |_| {
                if matches!(held.get_untracked(), Hold::Pressed(_)) {
                    let_go();
                }
            }
            on:contextmenu=move |event| {
                event.prevent_default();

                if matches!(held.get_untracked(), Hold::Up) {
                    held.set(Hold::LeftDown(press()));
                } else {
                    let_go();
                }
            }
            // A click with detail 0 comes from the keyboard, which presses and
            // releases the button at once.
            on:click=move |event| {
                if event.detail() == 0 {
                    control.send(Input::Button(button));
                }
            }
        >
            <Icon paths />
        </button>
    }
}
