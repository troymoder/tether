use android::crashes::Kind;
use leptos::prelude::*;

use super::style;
use crate::app::icons;
use crate::ui::button::{Button, IconButton};
use crate::ui::dropdown::{Choice, Dropdown};
use crate::ui::search::SearchBox;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Filters(
    sought: RwSignal<String>,
    kind: RwSignal<Option<Kind>>,
    #[prop(into)] kinds: Signal<Vec<Choice<Option<Kind>>>>,
    #[prop(into)] clearable: Signal<bool>,
    #[prop(into)] on_clear: Callback<()>,
    #[prop(into)] on_again: Callback<()>,
) -> impl IntoView {
    let choose = move |chosen: Option<Kind>| kind.set(chosen);

    view! {
        <div class=style::row>
            <SearchBox sought label="Show only the ones like this" placeholder="Filter" />
            <Dropdown
                label="The kind of crash"
                choices=kinds
                chosen=Signal::derive(move || Some(kind.get()))
                on_choose=choose
                class=style::kind
            />
            <Button
                disabled=Signal::derive(move || !clearable.get())
                on:click=move |_| on_clear.run(())
            >
                "Clear"
            </Button>
            <IconButton paths=icons::FOLLOW label="Look again" on:click=move |_| on_again.run(()) />
        </div>
    }
}
