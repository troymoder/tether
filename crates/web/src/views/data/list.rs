use std::collections::HashMap;

use android::data::{Kind, Store};
use leptos::prelude::*;

use super::query::counting;
use super::{style, use_at};
use crate::app::{Global, Icon, Page, Picked, icons};
use crate::ui::load::use_load;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::search::{Empty, SearchBox, matches};

#[allow(unreachable_pub)]
#[component]
pub(super) fn StoreList(
    app: String,
    stores: Vec<Store>,
    open: Store,
    listing: RwSignal<bool>,
    sought: RwSignal<String>,
    /// None for a preferences file, which has no tables.
    tables: Option<Signal<Loadable<Vec<String>>>>,
    on_table: Callback<String>,
) -> impl IntoView {
    let router = expect_context::<Global>().router;
    let listed = |heading: &'static str, kind: Kind| {
        let files: Vec<Store> = stores
            .iter()
            .filter(|kept| kept.kind == kind)
            .cloned()
            .collect();

        (!files.is_empty()).then(|| {
            let rows = files
                .into_iter()
                .map(|kept| {
                    let folds = kind == Kind::Database;
                    let is_open = kept == open;
                    let name = kept.name.clone();
                    let to = Page::Database {
                        app: Some(app.clone()),
                        store: Some(kept.name.clone()),
                        table: None,
                    };
                    let under = tables.filter(|_| folds && is_open).map(|tables| {
                        let app = app.clone();

                        view! {
                            <Status
                                of=tables
                                empty=(Vec::is_empty, "No tables.")
                                class=style::part
                                let:names
                            >
                                <Tables app=app.clone() store=kept.clone() names sought on_table />
                            </Status>
                        }
                    });

                    view! {
                        <button
                            type="button"
                            class=style::file
                            class=(style::folds, folds)
                            class=(style::open, is_open)
                            aria-expanded=folds.then(|| is_open.to_string())
                            on:click=move |_| router.go(to.clone())
                        >
                            {folds
                                .then(|| {
                                    view! {
                                        <Icon paths=if is_open {
                                            icons::CHEVRON
                                        } else {
                                            icons::RIGHT
                                        } />
                                    }
                                })}
                            <span>{name}</span>
                        </button>
                        {under}
                    }
                })
                .collect_view();

            view! {
                <span class=style::part>{heading}</span>
                {rows}
            }
        })
    };

    view! {
        <nav
            class=style::stores
            class=(style::hidden, move || !listing.get())
            aria-label="What the app keeps"
        >
            <div>
                <SearchBox
                    sought
                    label="Show only the tables with this in their name"
                    placeholder="Filter tables"
                />
            </div>
            <div>
                {listed("Databases", Kind::Database)} {listed("Preferences", Kind::Preferences)}
            </div>
        </nav>
    }
}

#[allow(unreachable_pub)]
#[component]
fn Tables(
    app: String,
    store: Store,
    names: Vec<String>,
    sought: RwSignal<String>,
    on_table: Callback<String>,
) -> impl IntoView {
    let device = expect_context::<Picked>().device;
    let at = use_at();
    let asked = counting(&names);
    // By the table's name. One query counts them all, after the tables are
    // listed.
    let counts = use_load(
        || (),
        move |()| {
            let device = device.get_untracked();
            let app = app.clone();
            let store = store.clone();
            let asked = asked.clone();

            async move {
                let counted = device.query(&app, &store.name, &asked).await?;
                let counts: HashMap<String, String> = counted
                    .into_iter()
                    .skip(1)
                    .filter_map(|row| {
                        let mut row = row.into_iter();

                        Some((row.next()?, row.next()?))
                    })
                    .collect();

                Ok(counts)
            }
        },
    );
    let all = names.len();
    let names = StoredValue::new(names);
    let shown = Memo::new(move |_| {
        let sought = sought.get();

        names.with_value(|names| {
            names
                .iter()
                .filter(|name| matches(&sought, &[name.as_str()]))
                .cloned()
                .collect::<Vec<String>>()
        })
    });

    view! {
        <Empty
            all
            shown=Signal::derive(move || shown.with(Vec::len))
            none="No tables."
            class=style::part
        />
        {move || {
            shown
                .get()
                .into_iter()
                .map(|name| {
                    let this = name.clone();
                    let pressed = name.clone();
                    let counted = name.clone();

                    view! {
                        <button
                            type="button"
                            class=style::table
                            class=(
                                style::open,
                                move || at.with(|at| at.table.as_deref() == Some(this.as_str())),
                            )
                            on:click=move |_| on_table.run(pressed.clone())
                        >
                            <span>{name}</span>
                            <code>
                                {move || {
                                    counts
                                        .with(|counts| match counts {
                                            Loadable::Ready(counts) => counts.get(&counted).cloned(),
                                            Loadable::Loading | Loadable::Failed(_) => None,
                                        })
                                }}
                            </code>
                        </button>
                    }
                })
                .collect_view()
        }}
    }
}
