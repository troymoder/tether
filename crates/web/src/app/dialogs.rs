use leptos::prelude::*;
use stylance::classes;

use super::style;
use super::{Adding, Global};
use crate::ui;
use crate::ui::button::{Button, Tone};
use crate::ui::dialog::Dialog;
use crate::ui::tabs::Tabs;
use crate::views::devices::Connect;
use crate::views::settings::NewEmulator;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Sure(title: String, body: String, action: &'static str) -> impl IntoView {
    let ui = expect_context::<Global>().ui;

    view! {
        <Dialog title class=style::sure alert=true on_close=move |()| ui.answer(false)>
            <span class=classes!(ui::style::hint, style::body)>{body}</span>
            <div class=style::foot>
                <Button on:click=move |_| ui.answer(false)>"Cancel"</Button>
                <Button tone=Tone::Red on:click=move |_| ui.answer(true)>
                    {action}
                </Button>
            </div>
        </Dialog>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Lost() -> impl IntoView {
    view! {
        <Dialog title="Disconnected" class=style::lost alert=true attr:aria-busy="true">
            <span class=classes!(
                ui::style::hint, style::body
            )>"Reconnecting to the primary…"</span>
            <span class=style::bar aria-hidden="true"></span>
        </Dialog>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Virtual,
    Physical,
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Add() -> impl IntoView {
    let ui = expect_context::<Global>().ui;
    let pairing = ui
        .adding
        .with_untracked(|adding| matches!(adding, Some(Adding::Pairing(..))));
    let kind = RwSignal::new(if pairing {
        Kind::Physical
    } else {
        Kind::Virtual
    });

    view! {
        <Dialog title="Add device" on_close=move |()| ui.adding.set(None)>
            <Tabs
                tabs=vec![(Kind::Virtual, "Virtual"), (Kind::Physical, "Physical")]
                open=kind
                on_open=move |to| kind.set(to)
            />
            {move || match kind.get() {
                Kind::Physical => view! { <Connect /> }.into_any(),
                Kind::Virtual => view! { <NewEmulator /> }.into_any(),
            }}
        </Dialog>
    }
}
