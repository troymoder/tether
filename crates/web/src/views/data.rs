//! The app's own folder is only readable on a device that gives root, like
//! an emulator, or when the app was built debuggable.
//!
//! The view is three layers, each built again when what it is about
//! changes, so nothing of one app or store is left for the next:
//! [`Database`] picks the app, [`StoreNav`] lists that app's stores, and
//! `QueryPane` shows the open store. The open app, store and table are in
//! the address.

use leptos::prelude::*;

use crate::app::{Global, Page, Picked, icons};
use crate::link::ServerStoreFields;
use crate::ui::button::IconButton;
use crate::ui::dropdown::{Choice, Dropdown};
use crate::ui::load::use_load;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::pane::Pane;
use crate::views::logs::front::{self, use_front};

mod device;
mod list;
mod pane;
mod query;
mod stores;
mod table;

pub(crate) use device::Data;
use stores::StoreNav;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "data.module.scss"
);

/// Each is None for the one the view starts on.
#[derive(Clone, Debug, PartialEq)]
struct At {
    app: Option<String>,
    store: Option<String>,
    table: Option<String>,
}

fn use_at() -> Memo<At> {
    let router = expect_context::<Global>().router;

    router.held(|page| match page {
        Page::Database { app, store, table } => Some(At { app, store, table }),
        _ => None,
    })
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Database() -> impl IntoView {
    let Global { link, router, .. } = expect_context::<Global>();
    let device = expect_context::<Picked>().device;
    let front = use_front();
    let at = use_at();

    // The registry's apps, which are listed first.
    link.follow(|| Some(protocol::Topic::Apps));

    // The view is built again when the device comes back, so the apps are
    // asked for once.
    let packages = use_load(
        || (),
        move |()| {
            let device = device.get_untracked();

            async move { device.user_apps().await }
        },
    );
    let choices = Memo::new(move |_| {
        packages.get().map(|installed| {
            link.server.apps().with(|registry| {
                let registered = registry.iter().filter(|app| installed.contains(&app.id));
                let others = installed
                    .iter()
                    .filter(|id| !registry.iter().any(|app| app.id == **id));

                registered
                    .map(|app| {
                        let name = match &app.name {
                            Some(name) => format!("{name} · {}", app.id),
                            None => app.id.clone(),
                        };

                        Choice::new(app.id.clone(), name)
                    })
                    .chain(others.map(|id| Choice::new(id.clone(), id.clone())))
                    .collect::<Vec<Choice<String>>>()
            })
        })
    });
    // The open app: the one the address names while it is listed. Without
    // one it stays on the app it started on, which is the app in front if
    // that is listed and otherwise the first. None for a device with no
    // apps.
    let app = Memo::new(move |before: Option<&Loadable<Option<String>>>| {
        let named = at.with(|at| at.app.clone());
        let kept = before.cloned().and_then(Loadable::ready).flatten();

        choices.get().map(|choices| {
            let listed = |id: &String| choices.iter().any(|choice| choice.value == *id);
            let start = || {
                let front = untrack(|| front::package(front));
                let first = choices
                    .iter()
                    .find(|choice| Some(&choice.value) == front.as_ref())
                    .or(choices.first());

                first.map(|choice| choice.value.clone())
            };

            named
                .filter(listed)
                .or_else(|| kept.filter(listed))
                .or_else(start)
        })
    });
    let listing = RwSignal::new(true);

    view! {
        <Pane title="Database" class=style::data>
            <div class=style::row>
                <IconButton
                    paths=icons::SIDE
                    label=Signal::derive(move || {
                        if listing.get() { "Hide the tables" } else { "Show the tables" }.to_owned()
                    })
                    framed=true
                    class=style::side
                    attr:aria-pressed=move || listing.get().to_string()
                    on:click=move |_| listing.update(|shown| *shown = !*shown)
                />
                <Dropdown
                    label="The app"
                    placeholder="No app"
                    class=style::app
                    choices=Signal::derive(move || match choices.get() {
                        Loadable::Ready(choices) => choices,
                        // There is nothing to choose from until they are
                        // listed, and the line under the row says why.
                        Loadable::Loading | Loadable::Failed(_) => Vec::new(),
                    })
                    chosen=Signal::derive(move || app.get().ready().flatten())
                    on_choose=move |id: String| {
                        router.go(Page::Database {
                            app: Some(id),
                            store: None,
                            table: None,
                        })
                    }
                />
            </div>
            // Built again for each app, since the memo only changes with
            // the open app.
            <Status of=app empty=(Option::is_none, "No apps on the device.") let:app>
                {app.map(|app| view! { <StoreNav app listing /> })}
            </Status>
        </Pane>
    }
}
