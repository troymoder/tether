use android::data::{Kind, Store};
use leptos::prelude::*;

use super::list::StoreList;
use super::query::{TABLES, first_rows};
use super::style;
use super::table::Table;
use crate::app::{Global, Page, Picked};
use crate::ui;
use crate::ui::action::use_action;
use crate::ui::button::Button;
use crate::ui::load::use_load;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::pinned_table::PinnedTable;

/// The first row holds the names of the columns.
type Rows = Vec<Vec<String>>;

#[allow(unreachable_pub)]
#[component]
pub(super) fn QueryPane(
    app: String,
    store: Store,
    stores: Vec<Store>,
    listing: RwSignal<bool>,
    sought: RwSignal<String>,
) -> impl IntoView {
    let router = expect_context::<Global>().router;
    let device = expect_context::<Picked>().device;
    let is_database = store.kind == Kind::Database;
    let table = match router.page_untracked() {
        Page::Database { table, .. } => table,
        _ => None,
    };
    let of = StoredValue::new((app.clone(), store.clone()));
    let sql = RwSignal::new(table.as_deref().map(first_rows).unwrap_or_default());
    // The last answer stays while the store is asked again. None until the
    // first one is here.
    let answer = RwSignal::new(None::<Result<Rows, String>>);
    let (asking, ask) = use_action(None, move |()| {
        let device = device.get_untracked();
        let (app, store) = of.get_value();
        let asked = sql.get_untracked();

        async move {
            let said = match store.kind {
                Kind::Database => device.query(&app, &store.name, &asked).await,
                Kind::Preferences => device.preferences(&app, &store.name).await,
            };

            // The answer says how it went, so no toast does. The pane may
            // be gone by now.
            answer.try_set(Some(said));

            Ok(None::<String>)
        }
    });
    // None until something was asked. A memo, so asking again does not draw
    // the same rows again.
    let shown = Memo::new(move |_| match answer.get() {
        Some(answer) => Some(Loadable::from(answer)),
        None => asking.get().then_some(Loadable::Loading),
    });
    let tables = is_database.then(|| {
        use_load(
            || (),
            move |()| {
                let device = device.get_untracked();
                let (app, store) = of.get_value();

                async move {
                    let said = device.query(&app, &store.name, TABLES).await?;
                    let names: Vec<String> = said
                        .into_iter()
                        .skip(1)
                        .filter_map(|row| row.into_iter().next())
                        .collect();

                    Ok(names)
                }
            },
        )
    });
    let on_table = Callback::new(move |name: String| {
        let (app, store) = of.get_value();

        sql.set(first_rows(&name));
        router.go(Page::Database {
            app: Some(app),
            store: Some(store.name),
            table: Some(name),
        });

        ask.run(());
    });

    // A preferences file shows what is in it as it opens. A database shows
    // the table the address names, as a press on it does.
    if !is_database || table.is_some() {
        ask.run(());
    }

    let unasked = Signal::derive(move || is_database && sql.with(|sql| sql.trim().is_empty()));
    let under = move || {
        let rows = answer.with(|answer| match answer {
            Some(Ok(rows)) => Some(rows.len().saturating_sub(1)),
            _ => None,
        });

        match (is_database, rows) {
            (true, None) => Some("Ctrl+Enter runs it.".to_owned()),
            (true, Some(1)) => Some("Ctrl+Enter runs it. 1 row.".to_owned()),
            (true, Some(rows)) => Some(format!("Ctrl+Enter runs it. {rows} rows.")),
            (false, None) => None,
            (false, Some(1)) => Some("1 key.".to_owned()),
            (false, Some(keys)) => Some(format!("{keys} keys.")),
        }
    };
    let path = format!("shared_prefs/{}", store.name);

    view! {
        <StoreList app stores open=store listing sought tables on_table />
        // The same three parts for either kind of store, so switching
        // between them moves nothing.
        <form
            class=style::asked
            on:submit=move |event| {
                event.prevent_default();
                ask.run(());
            }
        >
            {if is_database {
                view! {
                    <textarea
                        class=style::ask
                        aria-label="What to ask the database, in SQL"
                        placeholder="Press a table, or ask in SQL."
                        spellcheck="false"
                        bind:value=sql
                        on:keydown=move |event| {
                            if event.key() == "Enter" && (event.ctrl_key() || event.meta_key()) {
                                event.prevent_default();
                                ask.run(());
                            }
                        }
                    ></textarea>
                }
                    .into_any()
            } else {
                view! { <code class=style::ask>{path}</code> }.into_any()
            }}
            <div class=style::row>
                <span class=ui::style::hint>{under}</span>
                <Button submit=true busy=asking disabled=unasked class=style::run>
                    {if is_database { "Run" } else { "Reload" }}
                </Button>
            </div>
            <PinnedTable class=style::grid min_width=460>
                {move || {
                    shown
                        .get()
                        .map(|answer| {
                            view! {
                                <Status
                                    of=Signal::stored(answer)
                                    class=style::wait
                                    failed=style::error
                                    let:rows
                                >
                                    <Table rows />
                                </Status>
                            }
                        })
                }}
            </PinnedTable>
        </form>
    }
}
