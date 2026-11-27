use leptos::prelude::*;
use stylance::classes;

use super::{Origin, kind, style, write};
use crate::app::Picked;
use crate::ui;
use crate::ui::button::Button;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::tag::Tag;

#[allow(unreachable_pub)]
#[component]
pub(super) fn DevicePane(mine: RwSignal<String>) -> impl IntoView {
    let board = expect_context::<Picked>().board;
    let text = board.text;
    let held = Signal::derive(move || match text.get() {
        Some(text) => Loadable::Ready(text),
        None => Loadable::Loading,
    });
    // Following has copied it already, and there is nothing to copy until
    // the device has sent it.
    let copied = Signal::derive(move || {
        board.following()
            || text.with(|text| {
                text.as_ref()
                    .is_none_or(|text| mine.with(|mine| mine == text))
            })
    });

    view! {
        <section class=classes!(ui::style::group, style::side) aria-label="On the device">
            <div>
                <span>"On the device"</span>
                {move || {
                    text.with(|text| text.as_deref().map(kind)).map(|kind| view! { <Tag>{kind}</Tag> })
                }}
            </div>
            <div class=style::text>
                <Status of=held loading="Reading…" let:text>
                    {text}
                </Status>
            </div>
            <div>
                <Button
                    class=style::act
                    disabled=copied
                    on:click=move |_| {
                        let Some(text) = text.get_untracked() else {
                            return;
                        };

                        write(&text);
                        board.note(Origin::Device, &text);
                        mine.set(text);
                    }
                >
                    "Copy to this computer"
                </Button>
            </div>
        </section>
    }
}
