use leptos::prelude::*;

use super::style;
use crate::app::Picked;
use crate::ui::action::use_action;
use crate::ui::button::{Button, Tone};

#[allow(unreachable_pub)]
#[component]
pub(super) fn Editor(
    at: String,
    editing: RwSignal<Option<(String, String)>>,
    changed: Trigger,
) -> impl IntoView {
    let device = expect_context::<Picked>().device;
    // None only as the editor closes.
    let text = move || editing.with(|editing| editing.as_ref().map(|(_, text)| text.clone()));
    let (saving, write) = use_action(Some("files"), move |()| {
        let device = device.get_untracked();
        let open = editing.get_untracked();

        async move {
            let Some((at, text)) = open else {
                return Ok(None);
            };

            device.write(&at, text.as_bytes()).await?;

            editing.try_set(None);
            changed.notify();

            Ok(Some(format!("Saved {at}")))
        }
    });

    view! {
        <div class=style::editor>
            <div>
                <code>{at}</code>
                <span>
                    <Button on:click=move |_| editing.set(None)>"Close"</Button>
                    <Button tone=Tone::Blue busy=saving on:click=move |_| write.run(())>
                        "Save to the device"
                    </Button>
                </span>
            </div>
            <textarea
                aria-label="What is in the file"
                spellcheck="false"
                prop:value=text
                on:input=move |event| {
                    let typed = event_target_value(&event);

                    editing
                        .update(|editing| {
                            if let Some((_, text)) = editing {
                                *text = typed;
                            }
                        });
                }
            ></textarea>
        </div>
    }
}
