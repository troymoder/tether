use android::tweaks::Tweak;
use leptos::prelude::*;

use super::{style, use_tweak};
use crate::ui;

#[allow(unreachable_pub)]
#[component]
pub(super) fn TweakSwitch(
    label: &'static str,
    hint: &'static str,
    on: Signal<bool>,
    tweak: fn(bool) -> Tweak,
    again: Trigger,
) -> impl IntoView {
    let (busy, set) = use_tweak(again);

    view! {
        <label class=style::tweak>
            <span class=style::what>
                <span>{label}</span>
                <span>{hint}</span>
            </span>
            <input
                type="checkbox"
                class=ui::style::switch
                prop:checked=on
                disabled=busy
                on:change=move |event| set.run(tweak(event_target_checked(&event)))
            />
        </label>
    }
}
