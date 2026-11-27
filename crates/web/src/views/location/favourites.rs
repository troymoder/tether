use leptos::prelude::*;
use protocol::{Place, Request};
use stylance::classes;

use super::style;
use crate::app::{Global, icons};
use crate::link::ServerStoreFields;
use crate::ui;
use crate::ui::button::IconButton;
use crate::ui::search::Empty;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Favourites(
    #[prop(into)] at: Signal<Option<(f64, f64)>>,
    #[prop(into)] on_go: Callback<(f64, f64)>,
) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let places = Memo::new(move |_| link.server.places().get());
    let count = Signal::derive(move || places.with(Vec::len));
    let row = move |place: Place| {
        let Place {
            name,
            latitude,
            longitude,
        } = place;
        let forgotten = name.clone();
        // True while the device is at this place, to five decimals.
        let here = move || {
            at.get().is_some_and(|(at_latitude, at_longitude)| {
                (at_latitude - latitude).abs() < 5e-6 && (at_longitude - longitude).abs() < 5e-6
            })
        };

        view! {
            <div class=style::place class=(style::here, here)>
                <button
                    type="button"
                    class=style::found
                    title="Put the device here"
                    on:click=move |_| on_go.run((latitude, longitude))
                >
                    <span>{name}</span>
                    <code>{format!("{latitude:.5}, {longitude:.5}")}</code>
                </button>
                <IconButton
                    paths=icons::CLOSE
                    label="Forget this place"
                    small=true
                    on:click=move |_| {
                        link.ask(Request::ForgetPlace {
                            name: forgotten.clone(),
                        })
                    }
                />
            </div>
        }
    };

    view! {
        <Empty all=count shown=count none="Star a place to keep it here." class=style::under />
        <Show when=move || count.get() != 0>
            <div class=classes!(
                ui::style::group, style::list
            )>
                // Keyed so a row keeps its own place when one before it is
                // forgotten. The coordinates are in the key so a place kept
                // again under its name is drawn anew.
                <For
                    each=move || places.get()
                    key=|place| {
                        (place.name.clone(), place.latitude.to_bits(), place.longitude.to_bits())
                    }
                    children=row
                />
            </div>
        </Show>
    }
}
