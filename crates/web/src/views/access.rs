//! There is one user and no accounts or tokens. A browser, a command line or
//! another tetherd is trusted by the fingerprint of its key once something
//! already trusted approves it (crates/server/src/trust.rs). Other instances
//! cannot be linked, so the primary, the tetherd that served the page, is
//! the only one.

use leptos::prelude::*;
use protocol::{ClientId, InstanceId, Request};
use stylance::classes;

use crate::app::Global;
use crate::link::{Link, ServerStoreFields};
use crate::ui;
use crate::ui::menu::{Item, MenuOf, use_menu};
use crate::ui::pane::Pane;
use crate::ui::renamable::Renaming;
use crate::ui::tabs::Tabs;

mod clients;
mod instances;
mod row;
mod settings;

use clients::{AskingClients, TrustedClients};
use instances::Instances;
pub(crate) use settings::Settings;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "access.module.scss"
);

/// `at` is in milliseconds since 1970.
pub(crate) fn ago(at: u64) -> String {
    let minutes = ((js_sys::Date::now() - at as f64) / 60_000.0).max(0.0) as u64;

    match minutes {
        0 => "just now".to_owned(),
        1 => "a minute ago".to_owned(),
        2..60 => format!("{minutes} minutes ago"),
        60..120 => "an hour ago".to_owned(),
        120..1440 => format!("{} hours ago", minutes / 60),
        1440..2880 => "yesterday".to_owned(),
        _ => format!("{} days ago", minutes / 1440),
    }
}

#[derive(Clone, PartialEq)]
enum Key {
    Client(String),
    Instance(InstanceId),
}

impl Key {
    /// Takes the connection because the handlers that call it have no context
    /// to read it from.
    fn name(&self, link: Link) -> String {
        match self {
            Key::Client(print) => link.client_name(&ClientId(print.clone())),
            Key::Instance(id) => link.instance_name(id),
        }
    }

    fn primary(&self, link: Link) -> bool {
        matches!(self, Key::Instance(id) if link.primary().as_ref() == Some(id))
    }
}

#[derive(Clone, Copy)]
struct Rows {
    chosen: RwSignal<Option<Key>>,
    renaming: Renaming<Key>,
    menu: MenuOf<Key>,
    remove: Callback<Key>,
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Access() -> impl IntoView {
    let link = expect_context::<Global>().link;
    let waiting = Memo::new(move |_| {
        link.server
            .access()
            .with(|access| access.iter().flat_map(|access| &access.asking).count())
    });
    let asks = RwSignal::new(waiting.get_untracked() > 0);

    view! {
        <Pane
            title="Access"
            class=style::access
            under=move || {
                view! {
                    <Tabs
                        tabs=vec![(false, "Trusted"), (true, "Approvals")]
                        open=asks
                        on_open=move |to_asks| asks.set(to_asks)
                        counts=Callback::new(move |to_asks| if to_asks { waiting.get() } else { 0 })
                    />
                }
            }
        >
            <div class=classes!(ui::style::pane_body, style::body)>
                <Show when=move || asks.get() fallback=|| view! { <Trusted /> }>
                    <span class=ui::style::hint>"Check the fingerprint before approving."</span>
                    <AskingClients />
                    <NoneAsking title="Instances" />
                </Show>
            </div>
        </Pane>
    }
}

#[allow(unreachable_pub)]
#[component]
fn Trusted() -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let remove = Callback::new(move |key: Key| {
        // The primary cannot be unlinked.
        if untrack(|| key.primary(link)) {
            return;
        }

        let name = untrack(|| key.name(link));

        match key {
            Key::Client(print) => {
                let body = if untrack(|| link.me(&print)) {
                    "This is the browser you're using. It will need approval again."
                } else {
                    "It will need approval again."
                };

                ui.confirm_then(format!("Revoke {name}?"), body, "Revoke", async move {
                    link.ask(Request::Revoke { print });
                });
            }
            Key::Instance(_) => ui.confirm_then(
                format!("Unlink {name}?"),
                "Its devices are no longer reached through it.",
                "Unlink",
                async move { link.unimplemented("unlink instance") },
            ),
        }
    });
    let rows = Rows {
        chosen: RwSignal::new(None),
        renaming: Renaming::new(),
        menu: use_menu(),
        remove,
    };

    view! {
        <TrustedClients rows />
        <Instances rows />
        {rows
            .menu
            .view(
                false,
                move |key| Some((key.name(link), menu_items(rows, key, key.primary(link)))),
            )}
    }
}

fn menu_items(rows: Rows, key: &Key, primary: bool) -> Vec<Item> {
    let renamed = key.clone();
    let mut items = vec![Item::Keyed {
        text: "Rename",
        keys: Some("F2"),
        red: false,
        on: Callback::new(move |()| rows.renaming.start(renamed.clone())),
    }];

    if !primary {
        let removed = key.clone();

        items.push(Item::Keyed {
            text: if matches!(key, Key::Client(_)) {
                "Revoke"
            } else {
                "Unlink"
            },
            keys: Some("Del"),
            red: true,
            on: Callback::new(move |()| rows.remove.run(removed.clone())),
        });
    }

    items
}

/// Other instances cannot be linked, so their section of the Approvals tab
/// is always empty.
#[allow(unreachable_pub)]
#[component]
fn NoneAsking(title: &'static str) -> impl IntoView {
    view! {
        <section>
            <h2>{title}</h2>
            <div class=classes!(ui::style::group, style::asks)>
                <p class=classes!(ui::style::hint, ui::style::said)>"Nothing waiting."</p>
            </div>
        </section>
    }
}
