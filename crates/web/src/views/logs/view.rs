use std::time::Duration;

use leptos::ev;
use leptos::prelude::*;

use super::device::Sources;
use super::device_log::use_device_log;
use super::filter_popup::FilterPopup;
use super::find::FindBar;
use super::frame::Frame;
use super::front::use_front;
use super::line_detail::LineDetail;
use super::log::Log;
use super::output_log::use_output_log;
use super::panes::{Panes, PickedLine, Whose};
use super::reading::Reading;
use super::sift::Sift;
use super::style;
use super::tick::use_tick;
use crate::app::{Global, Picked, icons};
use crate::ui;
use crate::ui::button::{Button, IconButton};
use crate::ui::find::Finding;
use crate::ui::kept::{KeptHere, kept_then};
use crate::ui::pane::Pane;
use crate::ui::search::SearchBox;
use crate::ui::segmented::Segmented;

const WRAP_KEY: &str = "log-wrap";

const TICK: Duration = Duration::from_millis(250);

/// The ticks between fetches of the emulator's output, which makes 2
/// seconds.
const OUTPUT_EVERY: u32 = 8;

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Logs() -> impl IntoView {
    // The view starts wrapped as it was left, so it waits for that.
    kept_then(WRAP_KEY, true, |wrap| view! { <Shown wrap /> })
}

#[component]
fn Shown(wrap: KeptHere<bool>) -> impl IntoView {
    let ui = expect_context::<Global>().ui;
    let from = expect_context::<Picked>().device;
    let sources = Memo::new(move |_| from.get().sources());
    let chosen = RwSignal::new(None::<Whose>);
    let whose = Memo::new(move |_| Whose::shown(chosen.get(), sources.get()));
    let wrapped = Signal::derive(move || wrap.get());
    let sift = Sift::new(use_front());
    let panes = Panes::new(whose, sift, wrapped);
    let reading = Reading::new(panes, sift);
    let device_log = use_device_log(panes, sift, reading);
    let output_log = use_output_log(panes, sift, reading);
    let finding = panes.finding;
    let sought = RwSignal::new(String::new());

    Effect::new(move |_| {
        wrapped.track();
        ui.theme.track();
        panes.draw();
    });

    // A change of filter or of tab sifts the lines again and jumps to the
    // end. The lines are kept outside signals, so nothing else sifts them. The
    // pause and the last search end with the sifting by themselves
    // (`Panes::sifting`).
    Effect::new(move |_| {
        panes.sifting.track();
        reading.show_latest();
    });

    use_tick(TICK, move |tick| {
        device_log.drain();
        reading.keep_end();

        if tick % OUTPUT_EVERY == 1 {
            output_log.fetch();
        }
    });

    // Empties the log in view on this page only. tetherd keeps its copy.
    let clear = move |()| {
        let (log, _) = panes.shown();

        if whose.get_untracked() == Whose::Emulator {
            output_log.skip_shown();
        }

        log.update_value(Log::clear);
        panes.search(None);
        panes.picked.set(None);
        reading.resume();
    };
    // Closing the find bar takes the mark off the line it found.
    let end_find = move |()| {
        finding.set(Finding::Closed);
        panes.draw();
    };
    let pressed = move |event: ev::MouseEvent| {
        let (log, frame) = panes.shown();

        let Some(number) = frame.number_at(&event) else {
            return;
        };

        let raw = log.with_value(|log| log.line(number).map(str::to_owned));

        panes.picked.set(raw.map(|raw| PickedLine {
            number,
            whose: whose.get_untracked(),
            raw,
        }));

        panes.draw();
    };
    let unpick = move |()| {
        panes.picked.set(None);
        panes.draw();
    };
    let is_paused = move || reading.is_paused();
    let hidden = move |of: Whose| Signal::derive(move || whose.get() != of);
    // A phone only has its own log.
    let tabs = move || {
        view! {
            <Segmented
                label="Whose log"
                class=style::whose
                hidden=Signal::derive(move || !sources.get().contains(Sources::OUTPUT))
                choices=vec![(Whose::Device, "Device"), (Whose::Emulator, "Emulator")]
                greyed=Callback::new(move |whose: Whose| {
                    whose == Whose::Device && !sources.get().contains(Sources::DEVICE)
                })
                chosen=whose
                on_choose=move |whose: Whose| chosen.set(Some(whose))
            />
        }
    };

    view! {
        <Pane title="Logs" class=style::view tools=tabs>
            <div class=ui::style::pane_body>
                <div class=style::tools>
                    <SearchBox sought=sift.text label="Show only the lines with this in them" placeholder="Filter" />
                    <FilterPopup
                        sift
                        running=device_log.running
                        disabled=Signal::derive(move || whose.get() == Whose::Emulator)
                        wrap=wrapped
                        on_wrap=move |wrapped: bool| wrap.set(wrapped)
                        on_clear=clear
                    />
                    <IconButton
                        paths=icons::SEARCH
                        label="Find · Ctrl+F"
                        framed=true
                        attr:aria-pressed=move || finding.with(Finding::is_open).to_string()
                        on:click=move |_| {
                            if finding.with_untracked(Finding::is_open) {
                                end_find(());
                            } else {
                                finding.set(Finding::Open(None));
                            }
                        }
                    />
                    // Always present so pausing moves nothing.
                    <IconButton
                        paths=Signal::derive(move || if is_paused() { icons::PLAY } else { icons::PAUSE })
                        label=Signal::derive(move || if is_paused() { "Resume" } else { "Pause" }.to_owned())
                        framed=true
                        lit=Signal::derive(is_paused)
                        attr:aria-pressed=move || is_paused().to_string()
                        on:click=move |_| {
                            if reading.is_paused_now() {
                                reading.resume();
                            } else {
                                reading.unpin();
                                reading.pause();
                            }
                        }
                    />
                </div>
                // No line stands here while the log is being reached, so the
                // log under it does not move as it comes.
                <Show when=move || {
                    whose.get() == Whose::Device
                        && (!sources.get().contains(Sources::DEVICE) || device_log.failed.with(Option::is_some))
                }>
                    <p class=ui::style::hint>
                        {move || device_log.failed.get().unwrap_or_else(|| "device not running".to_owned())}
                    </p>
                </Show>
                // Both logs stay mounted so neither loses its lines while the
                // other is in view.
                <div
                    class=style::frame
                    on:keydown=move |event: ev::KeyboardEvent| {
                        let finds = (event.ctrl_key() || event.meta_key())
                            && !event.alt_key()
                            && event.key().eq_ignore_ascii_case("f");

                        if finds {
                            event.prevent_default();
                            finding.update(Finding::open);
                        }
                    }
                >
                    <LogPane frame=panes.device_frame hidden=hidden(Whose::Device) reading on_press=pressed />
                    <LogPane frame=panes.output_frame hidden=hidden(Whose::Emulator) reading on_press=pressed />
                    <Show when=move || finding.with(Finding::is_open)>
                        <FindBar panes reading sought on_close=end_find />
                    </Show>
                </div>
                {move || {
                    panes
                        .picked
                        .get()
                        .map(|line| view! { <LineDetail line names=sift.names on_close=unpick /> })
                }}
            </div>
            <Show when=is_paused>
                <Button class=style::latest on:click=move |_| reading.resume()>
                    {move || match reading.paused.get() {
                        None | Some(0) => "Resume".to_owned(),
                        Some(1) => "Resume · 1 new line".to_owned(),
                        Some(held) => format!("Resume · {held} new lines"),
                    }}
                </Button>
            </Show>
        </Pane>
    }
}

#[component]
fn LogPane(
    frame: Frame,
    #[prop(into)] hidden: Signal<bool>,
    reading: Reading,
    #[prop(into)] on_press: Callback<ev::MouseEvent>,
) -> impl IntoView {
    view! {
        <div
            node_ref=frame.scroller
            class=style::log
            class=(style::hidden, move || hidden.get())
            role="log"
            aria-label="The log, in a terminal. Ctrl+F finds in it."
            tabindex="0"
            on:scroll=move |_| reading.scrolled()
            on:click=move |event| on_press.run(event)
            // Caught on the way down, since the terminal below stops the
            // wheel event so it does not scroll itself.
            on:wheel:capture=move |_| reading.by_hand()
            on:keydown=move |_| reading.by_hand()
            on:touchmove=move |_| reading.by_hand()
            on:pointerdown=move |_| reading.by_hand()
            // Dragging the scroll bar.
            on:pointermove=move |event| {
                if event.buttons() != 0 {
                    reading.by_hand();
                }
            }
        >
            <div node_ref=frame.height class=style::height>
                <div node_ref=frame.holder class=style::drawn></div>
            </div>
        </div>
    }
}
