//! The page asks the device itself what it has (`Apps`). tetherd keeps the
//! builds and installs one when asked.

use leptos::prelude::*;
use protocol::{App, Topic, Variant};

use crate::app::{Global, Picked};
use crate::link::ServerStoreFields;
use crate::ui;
use crate::ui::loadable::Status;
use crate::ui::menu::use_menu;
use crate::ui::pane::Pane;
use crate::ui::search::SearchBox;
use crate::ui::segmented::Segmented;

mod device;
pub(crate) mod icon;
pub(crate) mod installed;
mod labels;
mod list;
mod missing;
mod row;
pub(crate) mod status;

pub(crate) use device::{Apps, Features};
pub(crate) use installed::Installed;
use labels::Labels;
use list::OnDevice;
use missing::NotInstalled;
use row::{Entry, items};

stylance::import_style!(
    #[allow(unreachable_pub)]
    pub(crate) style,
    "apps.module.scss"
);

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Shown {
    All,
    Registry,
    Others,
}

pub(crate) fn called(app: &App) -> String {
    app.name.clone().unwrap_or_else(|| app.id.clone())
}

pub(crate) fn versioned(variant: &Variant) -> String {
    match &variant.version {
        Some(version) => format!("{version} ({})", variant.build),
        None => format!("build {}", variant.build),
    }
}

/// The last listing is kept for the moment between tetherd dropping the app
/// and the view dropping what shows it.
pub(crate) fn use_app(first: App) -> Memo<App> {
    let link = expect_context::<Global>().link;
    let id = first.id.clone();

    Memo::new(move |last: Option<&App>| {
        let listed = link.server.apps().at_key(id.clone()).try_get();

        listed
            .or_else(|| last.cloned())
            .unwrap_or_else(|| first.clone())
    })
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Apps() -> impl IntoView {
    let link = expect_context::<Global>().link;
    let installed = Installed::of(expect_context::<Picked>().id);
    let sought = RwSignal::new(String::new());
    let shown = RwSignal::new(Shown::All);
    let system_shown = RwSignal::new(false);
    let menu = use_menu::<Entry>();
    // The listed apps that only the device can name. The registry's own
    // have their names and icons from tetherd.
    let unnamed = Memo::new(move |_| {
        let registered = link.server.apps().get();
        let system_shown = system_shown.get();

        installed
            .packages()
            .ready()
            .into_iter()
            .flatten()
            .filter(|package| system_shown || !package.system)
            .filter(|package| registered.iter().all(|app| app.id != package.id))
            .collect::<Vec<_>>()
    });
    let labels = Labels::start(unnamed.into());
    // Only whether the device has answered, so the lists under it are built
    // once and not again each time the device lists its apps.
    let answered = Memo::new(move |_| installed.packages().map(|_| ()));

    link.follow(|| Some(Topic::Apps));

    view! {
        <Pane
            title="Apps"
            class=style::apps
            tools=move || {
                view! {
                    <SearchBox
                        sought
                        class=style::search
                        label="Show only the apps with this in their name or id"
                        placeholder="Search this device"
                    />
                }
            }
        >
            <div class=ui::style::pane_body>
                <div class=style::filters>
                    <Segmented
                        label="Which apps are listed"
                        choices=vec![
                            (Shown::All, "All"),
                            (Shown::Registry, "From the registry"),
                            (Shown::Others, "Others"),
                        ]
                        chosen=shown
                        on_choose=move |choice| shown.set(choice)
                    />
                    <label class=ui::style::switched>
                        <input type="checkbox" class=ui::style::switch bind:checked=system_shown />
                        <span>"System apps"</span>
                    </label>
                </div>
                <Status of=answered let:_answered>
                    <OnDevice sought shown system_shown installed labels menu />
                    <NotInstalled sought shown installed />
                </Status>
            </div>
            {menu.view(false, move |entry| Some((entry.name(), items(installed, entry))))}
        </Pane>
    }
}
