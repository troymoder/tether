use leptos::prelude::*;

use super::{Column, style};
use crate::app::{Icon, icons};

#[allow(unreachable_pub)]
#[component]
pub(super) fn HeaderFilter(
    title: &'static str,
    #[prop(into)] cut: Signal<Option<String>>,
    column: Column,
    listing: RwSignal<Option<Column>>,
) -> impl IntoView {
    view! {
        <span class=style::head>
            <button
                class=(style::lit, move || cut.with(Option::is_some))
                title=format!("Filter by {}", title.to_lowercase())
                aria-haspopup="listbox"
                aria-expanded=move || (listing.get() == Some(column)).to_string()
                on:click=move |_| {
                    listing.update(|open| *open = (*open != Some(column)).then_some(column))
                }
            >
                <span>{move || cut.get().unwrap_or_else(|| title.to_owned())}</span>
                <Icon paths=icons::CHEVRON />
            </button>
        </span>
    }
}
