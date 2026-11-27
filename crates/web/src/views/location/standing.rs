use leptos::prelude::*;
use stylance::classes;

use super::search::{around, seek};
use super::{keep, style};
use crate::app::{Global, Picked, icons};
use crate::ui;
use crate::ui::button::IconButton;
use crate::ui::load::use_load;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Standing(#[prop(into)] at: Signal<Option<(f64, f64)>>) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let put = expect_context::<Picked>().routes.put;
    // No name is looked up along a route, where the device moves every
    // second.
    let named = Memo::new(move |_| {
        at.get()
            .filter(|_| put.with(|put| put.is_none_or(|put| put.route.is_none())))
    });
    let address = use_load(
        move || named.get(),
        |named: Option<(f64, f64)>| async move {
            let Some((latitude, longitude)) = named else {
                return Ok(None);
            };

            let found = seek(&around(latitude, longitude)).await?;

            Ok(found.into_iter().next().and_then(|found| found.name))
        },
    );
    let address = move || address.get().ready().flatten();
    let now = move || {
        let Some((latitude, longitude)) = at.get() else {
            return view! {
                <div class=style::at>
                    <span>"It has not said where it is"</span>
                    <span class=classes!(
                        ui::style::hint, style::under
                    )>
                        "A device says once an app on it asks. Press the map to put it somewhere."
                    </span>
                </div>
            }
            .into_any();
        };

        let own = put
            .with(Option::is_none)
            .then(|| "from its own GPS".to_owned());
        let under: Vec<String> = address().into_iter().chain(own).collect();

        view! {
            <div class=style::at>
                <code>{format!("{latitude:.5}, {longitude:.5}")}</code>
                {(!under.is_empty())
                    .then(|| {
                        view! {
                            <span class=classes!(
                                ui::style::hint, style::under
                            )>{under.join(" · ")}</span>
                        }
                    })}
            </div>
        }
        .into_any()
    };

    view! {
        <div class=style::row>
            {now}
            <IconButton
                paths=icons::STAR
                label="Keep this place in the favourites"
                framed=true
                class=style::star
                disabled=Signal::derive(move || at.with(Option::is_none))
                on:click=move |_| {
                    if let Some((latitude, longitude)) = at.get_untracked() {
                        keep(link, untrack(address).as_deref(), latitude, longitude);
                    }
                }
            />
        </div>
    }
}
