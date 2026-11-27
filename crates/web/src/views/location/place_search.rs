use std::time::Duration;

use leptos::prelude::*;
use stylance::classes;

use super::search::{Found, called_like, coordinates, seek};
use super::{keep, style};
use crate::app::{Global, icons};
use crate::ui;
use crate::ui::button::IconButton;
use crate::ui::debounced::use_debounced;
use crate::ui::loadable::Status;
use crate::ui::search::SearchBox;

const TYPED: Duration = Duration::from_millis(300);

/// The letters from which typed text is searched for without Enter.
const LEAST: usize = 3;

#[allow(unreachable_pub)]
#[component]
pub(super) fn PlaceSearch(#[prop(into)] on_go: Callback<(f64, f64)>) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let sought = RwSignal::new(String::new());
    // The text Enter was pressed on, which is searched for however short
    // it is.
    let entered = RwSignal::new(None::<String>);
    // Coordinates are not searched for as typed. They move the device, so
    // they wait for Enter.
    let wanted = Signal::derive(move || {
        let text = sought.get().trim().to_owned();
        let least = if entered.with(|entered| entered.as_ref() == Some(&text)) {
            1
        } else {
            LEAST
        };

        (coordinates(&text).is_none() && text.chars().count() >= least).then_some(text)
    });
    let found = use_debounced(wanted, TYPED, |text: String| async move {
        seek(&called_like(&text)).await
    });
    let enter = move || {
        let text = sought.get_untracked().trim().to_owned();

        match coordinates(&text) {
            Some(place) => on_go.run(place),
            None => entered.set(Some(text)),
        }
    };
    let results = move |places: Vec<Found>| {
        let rows = places
            .into_iter()
            .map(|place| {
                let Found {
                    name,
                    latitude,
                    longitude,
                } = place;
                let shown = name
                    .clone()
                    .unwrap_or_else(|| format!("{latitude:.5}, {longitude:.5}"));

                view! {
                    <div class=style::place>
                        <button
                            type="button"
                            class=style::found
                            on:click=move |_| {
                                sought.set(String::new());
                                on_go.run((latitude, longitude));
                            }
                        >
                            {shown}
                        </button>
                        <IconButton
                            paths=icons::STAR
                            label="Keep this place in the favourites"
                            small=true
                            on:click=move |_| keep(link, name.as_deref(), latitude, longitude)
                        />
                    </div>
                }
            })
            .collect_view();

        view! { <div class=classes!(ui::style::group, style::list)>{rows}</div> }
    };

    view! {
        <form
            class=style::search
            on:submit=move |event| {
                event.prevent_default();
                enter();
            }
        >
            <SearchBox
                sought
                label="Search for a place"
                placeholder="Search for a place, or type coordinates"
            />
        </form>
        {move || {
            let found = found.get()?;

            Some(
                view! {
                    <Status of=Signal::stored(found) empty=(Vec::is_empty, "No places found.") let:places>
                        {results(places)}
                    </Status>
                },
            )
        }}
    }
}
