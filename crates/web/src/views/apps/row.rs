use android::apps::Package;
use leptos::ev;
use leptos::prelude::*;
use protocol::App;

use super::installed::Installed;
use super::labels::Labels;
use super::{called, style, use_app};
use crate::ui::menu::MenuOf;

mod actions;
mod menu;
mod registered;
mod unregistered;

use actions::RowActions;
pub(super) use menu::items;
use registered::Registered;
use unregistered::Unregistered;

#[derive(Clone, PartialEq)]
pub(super) struct Entry {
    pub(super) package: Package,
    pub(super) app: Option<App>,
}

impl Entry {
    /// Only registry apps have a name the page knows, so the others go by
    /// their id.
    pub(super) fn name(&self) -> String {
        self.app
            .as_ref()
            .map_or_else(|| self.package.id.clone(), called)
    }

    /// The row itself follows what its key leaves out, such as whether the app
    /// runs.
    pub(super) fn key(&self) -> (String, u64, String, bool) {
        (
            self.package.id.clone(),
            self.package.version,
            self.package.path.clone(),
            self.app.is_some(),
        )
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn AppRow(
    entry: Entry,
    installed: Installed,
    labels: Labels,
    menu: MenuOf<Entry>,
    #[prop(into)] front: Signal<Option<String>>,
) -> impl IntoView {
    let id = StoredValue::new(entry.package.id.clone());
    let live = Memo::new(move |_| id.with_value(|id| installed.package(id)));
    let running =
        Signal::derive(move || live.with(|live| live.as_ref().is_some_and(|live| live.running)));
    let launchable =
        Signal::derive(move || live.with(|live| live.as_ref().is_some_and(|live| live.launchable)));
    let in_front =
        Memo::new(move |_| id.with_value(|id| front.with(|front| front.as_ref() == Some(id))));
    let app = entry.app.clone().map(use_app);
    let package = entry.package.clone();
    let cells = match app {
        Some(app) => view! { <Registered app installed running /> }.into_any(),
        None => view! { <Unregistered package labels running /> }.into_any(),
    };
    let pointed = {
        let entry = entry.clone();

        move |event: ev::MouseEvent| menu.at_pointer(&event, entry.clone())
    };

    view! {
        <div
            class=style::row
            class=(style::front, move || in_front.get())
            on:contextmenu=pointed
        >
            {cells}
            <RowActions entry app installed menu running launchable />
        </div>
    }
}
