use leptos::prelude::*;
use stylance::classes;

use super::listed::Kind;
use super::style;
use crate::app::{Icon, icons};
use crate::ui::button;
use crate::ui::popup::Popup;

#[allow(unreachable_pub)]
#[component]
pub(super) fn StateFilter(
    hidden: RwSignal<Vec<Kind>>,
    count: Callback<Kind, usize>,
) -> impl IntoView {
    // The filter button shows only an icon, so this names it for screen
    // readers.
    let label = move || match Kind::ALL.len() - hidden.with(Vec::len) {
        4 => "All states".to_owned(),
        0 => "No states".to_owned(),
        some => format!("{some} of 4 states"),
    };

    let toggle = move |kind: Kind| {
        hidden.update(|hidden| match hidden.iter().position(|one| *one == kind) {
            Some(at) => {
                hidden.remove(at);
            }
            None => hidden.push(kind),
        });
    };

    view! {
        <span class=style::states>
            <Popup
                button=|| view! { <Icon paths=icons::FILTER /> }
                button_class=classes!(button::style::icon, button::style::framed, style::filter)
                lit=Signal::derive(move || hidden.with(|hidden| !hidden.is_empty()))
                button_label=Signal::derive(move || format!("Which states are listed: {}", label()))
                label="Which states are listed"
                class=style::states_list
            >
                {Kind::ALL
                    .into_iter()
                    .map(|kind| {
                        view! {
                            <button
                                role="checkbox"
                                aria-checked=move || {
                                    (!hidden.with(|hidden| hidden.contains(&kind))).to_string()
                                }
                                on:click=move |_| toggle(kind)
                            >
                                <span class=style::check aria-hidden="true">
                                    <svg viewBox="0 0 24 24" inner_html=icons::CHECK></svg>
                                </span>
                                <span class=style::kind>
                                    <span>{kind.name()}</span>
                                    {kind.covers().map(|covers| view! { <span>{covers}</span> })}
                                </span>
                                <span class=style::count>{move || count.run(kind)}</span>
                            </button>
                        }
                    })
                    .collect_view()}
            </Popup>
        </span>
    }
}
