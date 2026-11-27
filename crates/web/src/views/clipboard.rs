//! The clipboard is read and set through the device's screen, so the view
//! only works while that is shown. Only text is copied, since scrcpy carries
//! nothing else.
//!
//! Following copies the device's clipboard to this computer as it changes.
//! A page cannot watch the computer's clipboard, so the other direction is
//! the Paste button or Ctrl+V typed into the device.
//!
//! The pick's `Board` (board.rs) holds what the page knows of the
//! clipboard, and goes on with this view closed.

use leptos::prelude::*;
use stylance::classes;

use crate::app::Picked;
use crate::ui;
use crate::ui::pane::Pane;

mod board;
mod computer_pane;
mod device;
mod device_pane;
mod recent;

pub(crate) use board::Board;
pub(crate) use device::Clipboard;

use board::{Clip, Origin, write, writes_unasked};
use computer_pane::ComputerPane;
use device_pane::DevicePane;
use recent::RecentClips;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "clipboard.module.scss"
);

fn kind(text: &str) -> &'static str {
    if text.is_empty() { "Empty" } else { "Text" }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Clipboard() -> impl IntoView {
    let Picked {
        board,
        control: screen,
        ..
    } = expect_context::<Picked>();
    // Text typed or pasted here to put on the device.
    let mine = RwSignal::new(String::new());

    // The clipboard is read through the screen, so the device is asked for
    // it when the view opens and whenever the screen comes back.
    Effect::new(move |_| {
        if screen.live() {
            board.ask(None);
        }
    });

    view! {
        <Pane title="Clipboard" class=style::clipboard tools=move || view! { <FollowSwitch mine /> }>
            <div class=ui::style::pane_body>
                <Show
                    when=move || screen.live()
                    fallback=|| {
                        view! {
                            <p class=ui::style::hint>
                                "The clipboard is read and set through the device's screen, which is not being shown."
                            </p>
                        }
                    }
                >
                    <div class=style::pair>
                        <div>
                            <DevicePane mine />
                            <ComputerPane mine />
                        </div>
                    </div>
                    <span class=classes!(ui::style::hint, style::line)>
                        "Ctrl+V in the device pastes from this computer."
                    </span>
                    <RecentClips mine />
                </Show>
            </div>
        </Pane>
    }
}

#[component]
fn FollowSwitch(mine: RwSignal<String>) -> impl IntoView {
    let board = expect_context::<Picked>().board;
    let unasked = writes_unasked();

    view! {
        <label
            class=classes!(ui::style::switched, style::follow)
            title=if unasked {
                "Copy on the device, paste here"
            } else {
                "Not supported in this browser. Use Copy."
            }
        >
            <input
                type="checkbox"
                class=ui::style::switch
                aria-label="Follow the device"
                disabled=!unasked
                prop:checked=move || board.following()
                on:change=move |event| {
                    let on = event_target_checked(&event);

                    // Following starts from what the device holds now.
                    if on && let Some(text) = board.text.get_untracked() {
                        mine.set(text);
                    }

                    board.follow(on);
                }
            />
            <span>"Follow the device"</span>
        </label>
    }
}
