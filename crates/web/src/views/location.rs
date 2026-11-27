use std::time::Duration;

use leptos::prelude::*;
use protocol::{Place, Request, Topic, mention};

use crate::app::{Global, Picked, icons};
use crate::link::{Link, ServerStoreFields};
use crate::ui::action::{use_action, use_action_many};
use crate::ui::button::{Button, IconButton};
use crate::ui::kept::kept_then;
use crate::ui::poll::use_poll;

mod device;
mod favourites;
mod located;
mod map;
mod place_search;
mod route;
mod route_player;
mod search;
mod standing;
mod tiles;

pub(crate) use device::Location;
use favourites::Favourites;
use located::Put;
pub(crate) use located::Routes;
use map::Map;
use place_search::PlaceSearch;
use route_player::RoutePlayer;
use standing::Standing;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "location.module.scss"
);

const ASKED: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq)]
enum First {
    Waiting,
    /// None if it had been put somewhere from here by then, which is where
    /// the map is already.
    Said(Option<(f64, f64)>),
}

fn keep(link: Link, called: Option<&str>, latitude: f64, longitude: f64) {
    let name = called
        .and_then(|called| called.split(',').next())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(|| format!("{latitude:.4}, {longitude:.4}"), str::to_owned);

    link.ask(Request::KeepPlace(Place {
        name,
        latitude,
        longitude,
    }));
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Location() -> impl IntoView {
    let link = expect_context::<Global>().link;
    let Picked {
        id, device, routes, ..
    } = expect_context::<Picked>();
    // The device as a notice names it, which `Link::say` turns into its
    // name.
    let named = StoredValue::new(mention(&id));

    link.follow(|| Some(Topic::Places));

    // The device's place is polled while this view is open because the
    // device does not report moves.
    let said = use_poll(ASKED, move || {
        let device = device.get_untracked();

        async move {
            // Rounded to a metre, so a wavering GPS does not read as a new
            // place each time.
            let rounded = |number: f64| (number * 1e5).round() / 1e5;
            let (latitude, longitude) = device.whereabouts().await?;

            Ok((rounded(latitude), rounded(longitude)))
        }
    });
    let believed = Memo::new(move |_| said.get().ready());
    let at = Memo::new(move |_| {
        routes
            .put
            .get()
            .map(|put| (put.latitude, put.longitude))
            .or(believed.get())
    });
    // The world is made here since a searched or kept place moves it (`go`).
    let world = map::use_world(at.get_untracked());
    let first = Memo::new(move |first: Option<&First>| match first {
        Some(First::Said(place)) => First::Said(*place),
        Some(First::Waiting) | None => match believed.get() {
            Some(place) => First::Said(routes.put.with_untracked(Option::is_none).then_some(place)),
            None => First::Waiting,
        },
    });
    let places = Memo::new(move |_| link.server.places().get());
    let open = RwSignal::new(true);

    // The device's first place is an answer to the poll above, which no
    // handler hears of, so an effect moves the map to it.
    Effect::new(move |_| {
        if let First::Said(Some(place)) = first.get() {
            world.look_at(place);
        }
    });

    // A second place picked while the device is told of the first is not
    // dropped.
    let (_, put) = use_action_many(
        Some("location"),
        move |(latitude, longitude): (f64, f64)| {
            let device = device.get_untracked();
            let named = named.get_value();

            routes.stop();

            async move {
                let told = device.put_location(latitude, longitude).await;

                if told.is_ok() {
                    // Another device may have been picked meanwhile.
                    routes.put.try_set(Some(Put {
                        latitude,
                        longitude,
                        route: None,
                    }));
                }

                // The dot stayed where it was put while the device was told.
                world.settle();

                told.map(|()| format!("{named} is at {latitude:.5}, {longitude:.5}"))
            }
        },
    );
    let go = Callback::new(move |place: (f64, f64)| {
        world.look_at(place);
        put.run(place);
    });
    let (_, reset) = use_action(Some("location"), move |()| {
        let device = device.get_untracked();
        let named = named.get_value();

        routes.stop();

        async move {
            device.reset_location().await?;
            routes.put.try_set(None);

            Ok(format!("{named} is where its own GPS says"))
        }
    });

    view! {
        <section class=style::view>
            // The map waits for the tile source it was left on, which is kept
            // in this browser, so it never starts on another.
            {kept_then(
                map::KIND_KEY,
                0u32,
                move |kind| view! { <Map kind at world kept=places on_pick=put /> },
            )}
            <div class=style::card>
                <div class=style::row>
                    <h1>"Location"</h1>
                    {move || {
                        let shown = open.get();

                        view! {
                            <IconButton
                                paths=if shown { icons::LIFT } else { icons::CHEVRON }
                                label=if shown {
                                    "Hide the favourites and the route"
                                } else {
                                    "Show the favourites and the route"
                                }
                                attr:aria-expanded=shown.to_string()
                                on:click=move |_| open.update(|open| *open = !*open)
                            />
                        }
                    }}
                </div>
                <PlaceSearch on_go=go />
                <Standing at />

                <div class=style::part class=(style::hidden, move || !open.get())>
                    <h2>"Favourites"</h2>
                    <Favourites at on_go=go />
                </div>

                <div class=style::part class=(style::hidden, move || !open.get())>
                    <h2>"Route"</h2>
                    <RoutePlayer />
                </div>
            </div>
            <Button class=style::own on:click=move |_| reset.run(())>
                "Use its own GPS"
            </Button>
        </section>
    }
}
