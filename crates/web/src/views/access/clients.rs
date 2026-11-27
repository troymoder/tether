use leptos::prelude::*;
use protocol::{Access, Client, Request};
use stylance::classes;

use super::row::{Called, Selectable};
use super::{Key, Rows, ago, style};
use crate::app::Global;
use crate::link::ServerStoreFields;
use crate::ui;
use crate::ui::button::{Button, Tone};
use crate::ui::loadable::{Loadable, Status};
use crate::ui::tag::Tag;

/// A memo, so it changes only when these clients do and a focused row keeps
/// its focus while devices come and go.
fn use_clients(of: fn(&Access) -> &Vec<Client>) -> Memo<Loadable<Vec<Client>>> {
    let link = expect_context::<Global>().link;

    Memo::new(move |_| {
        link.server.access().with(|access| match access {
            Some(access) => Loadable::Ready(of(access).clone()),
            None => Loadable::Loading,
        })
    })
}

/// The key of a row is all it shows, so a row is made again when any of it
/// changes. A row reused for another client would keep that client's
/// buttons.
fn shown(client: &Client) -> (String, String, String, u64) {
    (
        client.print.clone(),
        client.name.clone(),
        client.kind.clone(),
        client.at,
    )
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn TrustedClients(rows: Rows) -> impl IntoView {
    let clients = use_clients(|access| &access.trusted);

    view! {
        <section>
            <h2>"Clients"</h2>
            <div class=ui::style::group>
                <Status of=clients empty=(Vec::is_empty, "None yet.") let:clients>
                    <For each=move || clients.clone() key=shown let:client>
                        <TrustedRow client rows />
                    </For>
                </Status>
            </div>
        </section>
    }
}

#[allow(unreachable_pub)]
#[component]
fn TrustedRow(client: Client, rows: Rows) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let key = Key::Client(client.print.clone());
    let print = client.print.clone();
    let me = move || link.me(&print);
    let revoked = key.clone();

    view! {
        <Selectable rows of=key.clone()>
            <span class=style::name>
                <span class=style::title>
                    <Called rows of=key name=Signal::stored(client.name) />
                </span>
                <Tag>{client.kind}</Tag>
                {move || me().then(|| view! { <Tag class=style::me>"This one"</Tag> })}
            </span>
            <Button quiet=true on:click=move |_| rows.remove.run(revoked.clone())>
                "Revoke"
            </Button>
            <code class=style::line>
                <span>{client.print}</span>
                <span>{format!("last used {}", ago(client.at))}</span>
            </code>
        </Selectable>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn AskingClients() -> impl IntoView {
    let clients = use_clients(|access| &access.asking);
    let waiting = move || {
        clients.with(|clients| matches!(clients, Loadable::Ready(clients) if !clients.is_empty()))
    };

    view! {
        <section>
            <h2>"Clients"</h2>
            <div class=classes!(ui::style::group, style::asks) class=(style::waiting, waiting)>
                <Status of=clients empty=(Vec::is_empty, "Nothing waiting.") let:clients>
                    <For each=move || clients.clone() key=shown let:client>
                        <AskingRow client />
                    </For>
                </Status>
            </div>
        </section>
    }
}

#[allow(unreachable_pub)]
#[component]
fn AskingRow(client: Client) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let rejected = client.print.clone();
    let approved = client.print.clone();
    let print = client.print.clone();
    let me = move || link.me(&print);

    view! {
        <div class=style::row>
            <span class=style::name>
                <span class=style::title>
                    <span>{client.name}</span>
                </span>
                <Tag>{client.kind}</Tag>
                {move || me().then(|| view! { <Tag class=style::me>"This one"</Tag> })}
            </span>
            <span class=style::actions>
                <Button small=true on:click=move |_| link.ask(Request::Reject { print: rejected.clone() })>
                    "Reject"
                </Button>
                <Button
                    small=true
                    tone=Tone::Blue
                    on:click=move |_| link.ask(Request::Approve { print: approved.clone() })
                >
                    "Approve"
                </Button>
            </span>
            <code class=style::line>
                <span>{client.print}</span>
                <span>{format!("asked {}", ago(client.at))}</span>
            </code>
        </div>
    }
}
