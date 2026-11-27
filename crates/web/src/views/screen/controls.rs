use leptos::html;
use leptos::prelude::*;

use super::keys::stay;
use super::record::{Recording, screenshot};
use super::stats::StatsStrip;
use super::stream::Stream;
use super::{Features, geometry, style};
use crate::app::{Icon, Picked, icons};
use crate::device::Views;
use crate::ui::button::{self, IconButton};
use crate::ui::kept::{KeptSignal, kept_signal};

pub(super) const FULL: u32 = 100;

const STATS_KEY: &str = "device-stats";

#[allow(unreachable_pub)]
#[component]
pub(super) fn ControlsBar(
    video: NodeRef<html::Video>,
    canvas: NodeRef<html::Canvas>,
    stream: Stream,
    recording: Recording,
    features: Memo<Features>,
    sound_on: RwSignal<bool>,
    volume: KeptSignal<u32>,
) -> impl IntoView {
    let Picked {
        device, control, ..
    } = expect_context::<Picked>();
    let live = move || control.live();
    // The stats only show over a live screen, which arrives long after the
    // kept value is read.
    let stats_kept = kept_signal(STATS_KEY, false);
    let stats_shown = move || stats_kept.get() == Some(true);
    let stats_open = move || stats_shown() && live();
    let turned = Memo::new(move |_| control.turned());
    let auto = move || turned.get().and_then(|turned| turned.auto);
    let clipboard = Memo::new(move |_| device.get().views().get().contains(Views::CLIPBOARD));
    let sound_label = move || {
        if sound_on.get() {
            "Stop playing the device's sound"
        } else {
            "Play the device's sound"
        }
    };

    view! {
        <div class=style::controls class=(style::open, stats_open) on:mousedown=stay>
            <IconButton
                paths=icons::SCREENSHOT
                label="Screenshot"
                disabled=Signal::derive(move || !live())
                on:click=move |_| screenshot(video, canvas)
            />
            <button
                class=button::style::icon
                class=(style::recording, move || recording.on())
                class=(button::style::active, move || recording.on())
                title="Record"
                aria-label="Record"
                aria-pressed=move || recording.on().to_string()
                disabled=move || !live() && !recording.on()
                on:click=move |_| recording.toggle(video)
            >
                <Icon paths=icons::RECORD />
            </button>
            <Show when=move || features.get().contains(Features::ROTATES)>
                <IconButton
                    paths=icons::ROTATE
                    label="Rotate"
                    disabled=Signal::derive(move || !live() || turned.get().is_none())
                    on:click=move |_| {
                        if let Some(turned) = turned.get_untracked() {
                            stream.turn(Some(geometry::next_turn(turned.degrees)));
                        }
                    }
                />
                <button
                    class=button::style::icon
                    class=(button::style::active, move || auto() == Some(true))
                    title="Follow the screen"
                    aria-label="Follow the screen"
                    aria-pressed=move || (auto() == Some(true)).to_string()
                    disabled=move || !live() || auto().is_none()
                    on:click=move |_| {
                        let Some(turned) = turned.get_untracked() else {
                            return;
                        };

                        // Stops following by locking the screen at its
                        // current rotation.
                        if turned.auto == Some(true) {
                            stream.turn(Some(turned.degrees));
                        } else {
                            stream.turn(None);
                        }
                    }
                >
                    <Icon paths=icons::FOLLOW />
                </button>
            </Show>
            <button
                class=button::style::icon
                class=(button::style::active, stats_open)
                title="Video stats"
                aria-label="Video stats"
                aria-pressed=move || stats_shown().to_string()
                disabled=move || !live()
                on:click=move |_| stats_kept.set(stats_kept.get_untracked() != Some(true))
            >
                <Icon paths=icons::STATS />
            </button>
            <Show when=move || clipboard.get()>
                <CopyButton />
            </Show>
            <span class=style::rule aria-hidden="true"></span>
            <button
                class=button::style::icon
                class=(button::style::active, move || sound_on.get())
                title=sound_label
                aria-label=sound_label
                aria-pressed=move || sound_on.get().to_string()
                disabled=move || !live()
                on:click=move |_| sound_on.update(|on| *on = !*on)
            >
                {move || {
                    let paths = if sound_on.get() { icons::SOUND } else { icons::MUTED };

                    view! { <Icon paths /> }
                }}
            </button>
            // The slider is always there so the bar keeps its size.
            <input
                type="range"
                class=style::loud
                min="0"
                max="100"
                aria-label="Volume"
                prop:value=move || volume.get().unwrap_or(FULL).to_string()
                disabled=move || !sound_on.get()
                on:input=move |event| {
                    if let Ok(now) = event_target_value(&event).parse::<u32>() {
                        volume.set(now.min(FULL));
                    }
                }
            />
            <Show when=stats_open>
                <StatsStrip counted=stream.counted />
            </Show>
        </div>
    }
}

#[component]
fn CopyButton() -> impl IntoView {
    let Picked { board, control, .. } = expect_context::<Picked>();

    view! {
        <span class=style::copy>
            <IconButton
                paths=icons::COPY
                label="Copy device clipboard"
                disabled=Signal::derive(move || !control.live())
                on:click=move |_| board.take()
            />
            <Show when=move || board.copied()>
                <span class=style::copied role="status">"Copied"</span>
            </Show>
        </span>
    }
}
