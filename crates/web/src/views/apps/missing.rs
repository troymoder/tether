use leptos::prelude::*;
use protocol::{App, Platform};
use stylance::classes;

use super::icon::{AppIcon, use_registry_icon};
use super::installed::{Installed, Standing};
use super::{Shown, called, style, use_app, versioned};
use crate::app::{Global, Picked};
use crate::link::ServerStoreFields;
use crate::ui::button::{Button, Tone};
use crate::ui::loadable::Loadable;
use crate::ui::search::matches;

#[derive(Clone, PartialEq)]
enum Missing {
    /// Every build in the registry is an APK, so a device that is not an
    /// Android one is offered none.
    NotOffered,
    Apps(Vec<App>),
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn NotInstalled(
    #[prop(into)] sought: Signal<String>,
    #[prop(into)] shown: Signal<Shown>,
    installed: Installed,
) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let Picked { id, listed, .. } = expect_context::<Picked>();
    let android = id.platform() == Platform::Android;
    let missing = Memo::new(move |_| {
        let Some(packages) = installed.packages().ready() else {
            return Missing::NotOffered;
        };

        if shown.get() == Shown::Others || !android {
            return Missing::NotOffered;
        }

        let sought = sought.get();
        let mut apps: Vec<App> = link.server.apps().with(|apps| {
            apps.iter()
                .filter(|app| !packages.iter().any(|package| package.id == app.id))
                .filter(|app| matches(&sought, &[&called(app), &app.id]))
                .cloned()
                .collect()
        });

        apps.sort_by_cached_key(|app| called(app).to_lowercase());

        Missing::Apps(apps)
    });
    let any =
        move || missing.with(|missing| matches!(missing, Missing::Apps(apps) if !apps.is_empty()));
    let apps = move || match missing.get() {
        Missing::Apps(apps) => Some(apps),
        Missing::NotOffered => None,
    };
    let abis = move || {
        listed.with(|listed| {
            let described = listed.described()?;

            Some(view! { <code class=style::for_abis>{format!("for {}", described.abis.join(", "))}</code> })
        })
    };

    view! {
        <Show when=any>
            <div class=classes!(style::heading, style::later)>
                <h2>"In the registry, not installed"</h2>
                {abis}
            </div>
            <div class=classes!(style::grid, style::missing)>
                <For each=move || apps().into_iter().flatten() key=|app| app.id.clone() let:app>
                    <MissingRow app installed />
                </For>
            </div>
        </Show>
    }
}

#[allow(unreachable_pub)]
#[component]
fn MissingRow(app: App, installed: Installed) -> impl IntoView {
    let listed = expect_context::<Picked>().listed;
    let app = use_app(app);
    let id = app.with_untracked(|app| app.id.clone());
    let drawn = use_registry_icon(move || Some(app.get()));
    let about = move || {
        app.with(|app| {
            listed.with(|listed| {
                let abis = listed
                    .described()
                    .map(|described| described.abis.as_slice());
                let variant = abis.and_then(|abis| app.variant(abis));
                let target = abis.and_then(|abis| app.target(abis));
                // The build's own first ABI where none fits, so the row says
                // what there is.
                let abi = target.map(str::to_owned).or_else(|| {
                    let newest = app.latest()?;

                    Some(newest.abis.first().map_or("any", String::as_str).to_owned())
                });
                let version = variant.or_else(|| app.latest()).map(versioned);

                [version, abi]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
        })
    };
    let fits = Memo::new(move |_| {
        listed.with(|listed| {
            listed
                .described()
                .is_some_and(|described| app.with(|app| app.variant(&described.abis).is_some()))
        })
    });
    let installing = Memo::new(move |_| {
        app.with(|app| installed.standing(app)) == Loadable::Ready(Standing::Installing)
    });

    view! {
        <div class=style::row>
            <AppIcon id=id.clone() drawn />
            <span class=style::names>
                <span class=style::title>
                    <strong>{move || app.with(called)}</strong>
                    <code class=style::version>{about}</code>
                </span>
                <span class=style::id>
                    <code>{id}</code>
                </span>
            </span>
            <span class=style::actions>
                <Button
                    tone=Tone::Blue
                    small=true
                    busy=installing
                    disabled=Signal::derive(move || !fits.get())
                    attr:title=move || (!fits.get()).then_some("No build for this device")
                    on:click=move |_| app.with_untracked(|app| installed.install(app))
                >
                    {move || if installing.get() { "Installing…" } else { "Install" }}
                </Button>
            </span>
        </div>
    }
}
