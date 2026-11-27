use leptos::ev;
use leptos::prelude::*;
use stylance::classes;
use wasm_bindgen::JsCast;

use super::device_button::DeviceButton;
use super::waiting::Waiting;
use super::{Button, Features, style};
use crate::app::{Global, Icon, Picked, icons};
use crate::ui::button;

/// Keeps keyboard focus on the device after a button around it is pressed.
/// A slider is left alone so it can still be dragged.
pub(super) fn stay(event: ev::MouseEvent) {
    let slider = event
        .target()
        .is_some_and(|target| target.dyn_ref::<web_sys::HtmlInputElement>().is_some());

    if !slider {
        event.prevent_default();
    }
}

fn device_button(
    features: Memo<Features>,
    button: Button,
    title: &'static str,
    paths: &'static str,
) -> impl IntoView {
    view! {
        <Show when=move || features.get().contains(Features::button(button))>
            <DeviceButton button title paths />
        </Show>
    }
}

/// The row stays even when a device has no face buttons, so the screen keeps
/// its place.
#[allow(unreachable_pub)]
#[component]
pub(super) fn NavKeys(features: Memo<Features>) -> impl IntoView {
    view! {
        <div class=style::nav_keys on:mousedown=stay>
            {device_button(features, Button::Back, "Back", icons::BACK)}
            {device_button(features, Button::Home, "Home", icons::HOME)}
            {device_button(features, Button::Recents, "Recents", icons::RECENTS)}
        </div>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn SideKeys(
    features: Memo<Features>,
    resting: Memo<(Waiting, String)>,
) -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let id = StoredValue::new(expect_context::<Picked>().id);
    let off = Memo::new(move |_| {
        resting.with(|(how, name)| (*how == Waiting::Off).then(|| name.clone()))
    });
    let power = move || match off.get() {
        Some(name) => {
            let label = format!("Boot {name}");

            view! {
                <button
                    class=classes!(button::style::icon, style::boot)
                    title=label.clone()
                    aria-label=label
                    on:click=move |_| link.connect_to(ui, id.get_value(), None)
                >
                    <Icon paths=icons::POWER />
                </button>
            }
            .into_any()
        }
        None => device_button(features, Button::Power, "Power", icons::POWER).into_any(),
    };

    view! {
        <div class=style::keys on:mousedown=stay>
            {power}
            {device_button(features, Button::VolumeUp, "Volume up", icons::LOUDER)}
            {device_button(features, Button::VolumeDown, "Volume down", icons::QUIETER)}
        </div>
    }
}
