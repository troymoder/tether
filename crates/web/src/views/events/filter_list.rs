use leptos::prelude::*;

use super::{Column, style};
use crate::ui::loadable::{Loadable, Status};
use crate::ui::popbox::{Open, PopBox};

#[allow(unreachable_pub)]
#[component]
pub(super) fn FilterList<T>(
    label: &'static str,
    column: Column,
    listing: RwSignal<Option<Column>>,
    #[prop(into)] values: Signal<Loadable<Vec<(T, String)>>>,
    // It takes no `into`, which would leave `T` open between a value and an
    // Option of it.
    cut: Signal<Option<T>>,
    #[prop(into)] on_cut: Callback<Option<T>>,
    under: &'static str,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let open = Open::new(
        Signal::derive(move || listing.get() == Some(column)),
        Callback::new(move |()| listing.set(None)),
    );
    let option = move |value: Option<T>, name: String| {
        let chosen = cut.with(|cut| *cut == value);

        view! {
            <button
                type="button"
                role="option"
                aria-selected=chosen.to_string()
                on:click=move |_| {
                    on_cut.run(value.clone());
                    listing.set(None);
                }
            >
                {name}
            </button>
        }
    };

    view! {
        <PopBox open label class=under>
            <span class=style::list role="listbox">
                {move || option(None, "All".to_owned())}
                <Status of=values let:values>
                    {values
                        .into_iter()
                        .map(|(value, name)| option(Some(value), name))
                        .collect_view()}
                </Status>
            </span>
        </PopBox>
    }
}
