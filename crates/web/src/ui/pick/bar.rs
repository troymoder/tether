use leptos::prelude::*;

use super::style;
use crate::app::{Icon, icons};
use crate::ui::button::{Button, IconButton, Tone};

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Tick(
    #[prop(into)] picked: Signal<bool>,
    label: String,
    /// Sets its size and position.
    class: &'static str,
) -> impl IntoView {
    view! {
        <button
            type="button"
            class=stylance::classes!(style::tick, class)
            role="checkbox"
            aria-checked=move || picked.get().to_string()
            aria-label=label
        >
            <Icon paths=icons::CHECK />
        </button>
    }
}

/// The view has to name a container `fsel`, which the bar sizes against.
#[allow(unreachable_pub)]
#[component]
pub(crate) fn PickedBar(
    picked: RwSignal<Vec<String>>,
    #[prop(into)] shown: Signal<Vec<String>>,
    /// The label of the button that picks all of them.
    all: &'static str,
    #[prop(into)] on_download: Callback<()>,
    #[prop(into)] on_delete: Callback<()>,
) -> impl IntoView {
    let everything_picked = move || {
        shown.with(|shown| picked.with(|picked| shown.iter().all(|path| picked.contains(path))))
    };

    view! {
        <Show when=move || picked.with(|picked| !picked.is_empty())>
            <div class=style::bar role="toolbar" aria-label="What to do with the picked ones">
                <span role="status">{move || format!("{} picked", picked.with(Vec::len))}</span>
                <Button
                    class=style::act
                    disabled=Signal::derive(everything_picked)
                    on:click=move |_| {
                        picked
                            .update(|picked| {
                                for path in shown.get_untracked() {
                                    if !picked.contains(&path) {
                                        picked.push(path);
                                    }
                                }
                            });
                    }
                >
                    <span class=style::long>{all}</span>
                    <span class=style::short>"All"</span>
                </Button>
                <Button
                    class=style::signed
                    attr:title="Download the picked ones"
                    attr:aria-label="Download the picked ones"
                    on:click=move |_| on_download.run(())
                >
                    <Icon paths=icons::DOWNLOAD />
                    <span class=style::long>"Download"</span>
                </Button>
                <Button
                    tone=Tone::Red
                    class=style::signed
                    attr:title="Delete the picked ones from the device"
                    attr:aria-label="Delete the picked ones from the device"
                    on:click=move |_| on_delete.run(())
                >
                    <Icon paths=icons::BIN />
                    <span class=style::long>"Delete"</span>
                </Button>
                <IconButton
                    paths=icons::CLOSE
                    label="Unpick all"
                    class=style::unpick
                    on:click=move |_| picked.set(Vec::new())
                />
            </div>
        </Show>
    }
}
