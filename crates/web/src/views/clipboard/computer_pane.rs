use leptos::prelude::*;
use stylance::classes;
use wasm_bindgen_futures::JsFuture;

use super::{Origin, kind, style};
use crate::app::Picked;
use crate::ui;
use crate::ui::action::use_action;
use crate::ui::button::{Button, Tone};
use crate::ui::tag::Tag;

#[allow(unreachable_pub)]
#[component]
pub(super) fn ComputerPane(mine: RwSignal<String>) -> impl IntoView {
    let board = expect_context::<Picked>().board;
    // A page can only read this computer's clipboard when it is asked to.
    let (pasting, paste) = use_action(None, move |()| {
        let asked = window().navigator().clipboard().read_text();

        async move {
            let text = JsFuture::from(asked)
                .await
                .ok()
                .and_then(|text| text.as_string())
                .ok_or("clipboard failed: the browser did not allow reading it")?;

            mine.try_set(text);

            Ok(None::<String>)
        }
    });
    let same = Signal::derive(move || {
        mine.with(|mine| {
            mine.trim().is_empty() || board.text.with(|text| text.as_ref() == Some(mine))
        })
    });

    view! {
        <section class=classes!(ui::style::group, style::side) aria-label="On this computer">
            <div>
                <span>"On this computer"</span>
                <Tag>{move || mine.with(|mine| kind(mine))}</Tag>
            </div>
            <textarea
                class=style::text
                aria-label="What to put on the device's clipboard"
                placeholder="Type or paste here"
                spellcheck="false"
                bind:value=mine
            ></textarea>
            <div>
                <span>
                    <Button
                        class=style::act
                        busy=pasting
                        attr:title="Reads this computer's clipboard once, now"
                        attr:aria-label="Paste from this computer's clipboard"
                        on:click=move |_| paste.run(())
                    >
                        "Paste"
                    </Button>
                    <Button
                        quiet=true
                        class=style::clear
                        disabled=Signal::derive(move || mine.with(String::is_empty))
                        on:click=move |_| mine.set(String::new())
                    >
                        "Clear"
                    </Button>
                </span>
                <Button
                    tone=Tone::Blue
                    class=style::put
                    disabled=same
                    on:click=move |_| board.put(Origin::Computer, mine.get_untracked())
                >
                    "Put on the device"
                </Button>
            </div>
        </section>
    }
}
