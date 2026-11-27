use leptos::prelude::*;
use leptos::task::spawn_local;
use protocol::mention;
use stylance::classes;

use super::located::Speed;
use super::{route, style};
use crate::app::{Global, Picked};
use crate::link;
use crate::ui;
use crate::ui::button::{Button, Tone};
use crate::ui::dropdown::{Choice, Dropdown};
use crate::ui::file_pick::{FilePick, FilePicker};

#[allow(unreachable_pub)]
#[component]
pub(super) fn RoutePlayer() -> impl IntoView {
    let link = expect_context::<Global>().link;
    let Picked {
        id, device, routes, ..
    } = expect_context::<Picked>();
    // The device as a notice names it, which `Link::say` turns into its
    // name.
    let named = StoredValue::new(mention(&id));
    let about = StoredValue::new(id);
    let on_route = move || routes.put.get().and_then(|put| put.route);
    let picker = FilePicker::new();
    let chosen = RwSignal::new_local(None::<web_sys::File>);
    let speed = RwSignal::new(Speed::Walking);
    let speeds: Vec<Choice<Speed>> = Speed::ALL
        .into_iter()
        .map(|speed| Choice::new(speed, speed.name()))
        .collect();

    let play = move || {
        let Some(chosen) = chosen.get_untracked() else {
            link.say("select a GPX or KML file");
            return;
        };

        let device = device.get_untracked();
        let speed = speed.get_untracked();
        let named = named.get_value();
        let about = about.get_value();

        spawn_local(async move {
            let Some(text) = link::read(&chosen).await else {
                link.say(format!("read failed: {}", chosen.name()));
                return;
            };

            let route = route::points(&String::from_utf8_lossy(&text));

            if route.len() < 2 {
                link.say("no track found: expected GPX or KML");
                return;
            }

            let length = route::length(&route);

            link.did(
                "location",
                Some(&about),
                Ok(format!(
                    "{named} is on a route of {:.1} km, which takes {:.0} minutes",
                    length / 1000.0,
                    length / speed.metres_a_second() / 60.0
                )),
            );

            routes.follow(device, about, route, speed).await;
        });
    };

    view! {
        <div class=classes!(ui::style::group, style::route)>
            <FilePick
                picker
                accept=".gpx,.kml,.xml"
                label="The route's file"
                on_file=move |file: web_sys::File| chosen.set(Some(file))
            />
            <div class=style::row>
                <Button
                    class=style::file
                    attr:title="A GPX or a KML file with a track. The device is moved along it, a second at a time."
                    on:click=move |_| picker.open()
                >
                    {move || {
                        chosen
                            .with(|chosen| chosen.as_ref().map(web_sys::File::name))
                            .unwrap_or_else(|| "Choose a GPX or KML file".to_owned())
                    }}
                </Button>
                <Button
                    tone=Signal::derive(move || {
                        if on_route().is_some() { Tone::Red } else { Tone::Blue }
                    })
                    class=style::play
                    on:click=move |_| {
                        if untrack(on_route).is_some() {
                            routes.stop();
                            link.say("The route was stopped");
                        } else {
                            play();
                        }
                    }
                >
                    {move || if on_route().is_some() { "Stop" } else { "Play" }}
                </Button>
            </div>
            <Dropdown
                class=style::speed
                label="How fast"
                choices=speeds
                chosen=Signal::derive(move || Some(speed.get()))
                on_choose=move |chosen: Speed| speed.set(chosen)
            />
            {move || {
                on_route()
                    .map(|share| {
                        view! {
                            <div class=style::row>
                                <div
                                    class=style::progress
                                    role="progressbar"
                                    aria-label="How far along the route it is"
                                >
                                    <div style:width=format!("{:.1}%", share * 100.0)></div>
                                </div>
                                <code>{format!("{:.0}%", share * 100.0)}</code>
                            </div>
                        }
                    })
            }}
        </div>
    }
}
