use std::time::Duration;

use android::tweaks::{Tweak, Tweaks as Setting};
use leptos::prelude::*;

use crate::app::Picked;
use crate::ui;
use crate::ui::action::use_action;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::pane::Pane;
use crate::ui::poll::use_poll;

mod device;
mod language;
mod switch;
mod text_size;

pub(crate) use device::Tweaks;
use language::Language;
use switch::TweakSwitch;
use text_size::TextSize;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "preferences.module.scss"
);

/// The tweaks are polled while the view is open, because Android does not
/// report changes made on the device.
const ASKED: Duration = Duration::from_secs(5);

/// Each control has its own action, so a change on one control is not dropped
/// while another's is under way.
fn use_tweak(again: Trigger) -> (Signal<bool>, Callback<Tweak>) {
    let device = expect_context::<Picked>().device;

    use_action(Some("tweaks"), move |tweak: Tweak| {
        let device = device.get_untracked();

        async move {
            let said = device.tweak(&tweak).await;

            again.notify();

            said
        }
    })
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Preferences() -> impl IntoView {
    let device = expect_context::<Picked>().device;
    let again = Trigger::new();
    let known = use_poll(ASKED, move || {
        let device = device.get_untracked();

        again.track();

        async move { device.tweaks().await }
    });
    // Later answers update the controls without rebuilding them, so a control
    // keeps focus while it is used.
    let first = Memo::new_with_compare(
        move |_| known.get(),
        |was, is| {
            let ready = |one: Option<&Loadable<Setting>>| matches!(one, Some(Loadable::Ready(_)));

            !(ready(was) && ready(is))
        },
    );

    view! {
        <Pane title="Preferences">
            <div class=ui::style::pane_body>
                <Status of=first let:first>
                    <Controls first known again />
                </Status>
            </div>
        </Pane>
    }
}

#[allow(unreachable_pub)]
#[component]
fn Controls(first: Setting, known: Signal<Loadable<Setting>>, again: Trigger) -> impl IntoView {
    // Android cannot answer while it boots, so the last reading stays.
    let now = Memo::new(move |last: Option<&Setting>| {
        let read = known.get().ready();

        read.or_else(|| last.cloned())
            .unwrap_or_else(|| first.clone())
    });

    view! {
        <div class=style::tweaks>
            <h2>"Appearance"</h2>
            <div class=ui::style::group>
                <TweakSwitch
                    label="Dark theme"
                    hint="Apps follow it where they support it"
                    on=Signal::derive(move || now.with(|now| now.dark))
                    tweak=Tweak::Dark
                    again
                />
                <TweakSwitch
                    label="Bold text"
                    hint="Text is of its usual weight"
                    on=Signal::derive(move || now.with(|now| now.bold))
                    tweak=Tweak::Bold
                    again
                />
                <TweakSwitch
                    label="High contrast text"
                    hint="Text is outlined to stand off what is behind it"
                    on=Signal::derive(move || now.with(|now| now.high_contrast))
                    tweak=Tweak::HighContrast
                    again
                />
                <TweakSwitch
                    label="Demo status bar"
                    hint="A set time, full battery and signal, no notifications"
                    on=Signal::derive(move || now.with(|now| now.tidy_bar))
                    tweak=Tweak::TidyBar
                    again
                />
            </div>
            <h2>"Text"</h2>
            <div class=ui::style::group>
                <TextSize size=Signal::derive(move || now.with(|now| now.text_size)) again />
            </div>
            <h2>"Language"</h2>
            <Language
                locale=Signal::derive(move || now.with(|now| now.locale.clone()))
                rooted=Signal::derive(move || now.with(|now| now.rooted))
                again
            />
        </div>
    }
}
