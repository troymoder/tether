use leptos::prelude::*;
use stylance::classes;

use super::installed::Installed;
use super::labels::Labels;
use super::row::{AppRow, Entry};
use super::{Shown, style};
use crate::app::{Global, Picked};
use crate::link::ServerStoreFields;
use crate::ui::menu::MenuOf;
use crate::ui::search::{Empty, matches};
use crate::views::logs::front::{self, use_front};

/// The list is its rows up to this many, and fills the pane beyond.
const FEW: usize = 5;

#[allow(unreachable_pub)]
#[component]
pub(super) fn OnDevice(
    #[prop(into)] sought: Signal<String>,
    #[prop(into)] shown: Signal<Shown>,
    #[prop(into)] system_shown: Signal<bool>,
    installed: Installed,
    labels: Labels,
    menu: MenuOf<Entry>,
) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let listed = expect_context::<Picked>().listed;
    let front = use_front();
    let front = Memo::new(move |_| front::package(front));
    let entries = Memo::new(move |_| {
        let packages = installed.packages().ready();
        let apps = link.server.apps().get();
        let sought = sought.get();
        let shown = shown.get();
        let system_shown = system_shown.get();
        let mut entries: Vec<Entry> = packages
            .into_iter()
            .flatten()
            .filter(|package| system_shown || !package.system)
            .map(|package| Entry {
                app: apps.iter().find(|app| app.id == package.id).cloned(),
                package,
            })
            .filter(|entry| match shown {
                Shown::All => true,
                Shown::Registry => entry.app.is_some(),
                Shown::Others => entry.app.is_none(),
            })
            .filter(|entry| {
                let label = labels.name(&entry.package);
                let name = entry.name();
                let mut texts = vec![name.as_str(), entry.package.id.as_str()];

                texts.extend(label.as_deref());

                matches(&sought, &texts)
            })
            .collect();

        // The others stay in the order of their ids as their names arrive,
        // so rows do not jump about while they are read.
        entries.sort_by_cached_key(|entry| (entry.app.is_none(), entry.name().to_lowercase()));

        entries
    });
    let all = Signal::derive(move || installed.packages().ready().into_iter().flatten().count());
    let count = Memo::new(move |_| entries.with(Vec::len));
    let counted = move || match count.get() {
        1 => "1 app".to_owned(),
        count => format!("{count} apps"),
    };

    view! {
        <div class=style::heading>
            <h2>{move || format!("On {}", listed.with(|listed| listed.name.clone()))}</h2>
            <span class=style::counted>{counted}</span>
        </div>
        <Empty all shown=count none="No apps." />
        <div class=move || {
            classes!(style::grid, style::list, (count.get() <= FEW).then_some(style::few))
        }>
            <For each=move || entries.get() key=Entry::key let:entry>
                <AppRow entry installed labels menu front />
            </For>
        </div>
    }
}
