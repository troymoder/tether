use leptos::prelude::*;
use protocol::App;

use super::super::icon::{AppIcon, use_registry_icon};
use super::super::installed::Installed;
use super::super::status::StatusTag;
use super::super::{called, style, versioned};
use crate::app::Picked;
use crate::ui::auto_toggle::AutoToggle;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Registered(
    app: Memo<App>,
    installed: Installed,
    running: Signal<bool>,
) -> impl IntoView {
    let device = expect_context::<Picked>().id;
    let id = app.with_untracked(|app| app.id.clone());
    let drawn = use_registry_icon(move || Some(app.get()));
    let standing = Signal::derive(move || app.with(|app| installed.standing(app)));
    let version = move || {
        app.with(|app| {
            let build = installed.build(app)?;
            let variant = app.variants.iter().find(|variant| variant.build == build)?;

            Some(versioned(variant))
        })
    };

    view! {
        <AppIcon id=id.clone() drawn running />
        <span class=style::names>
            <span class=style::title>
                <strong>{move || app.with(called)}</strong>
                {move || {
                    version().map(|version| view! { <span class=style::version>{version}</span> })
                }}
            </span>
            <span class=style::id>
                <span class=style::registry>"Registry"</span>
                <code>{id.clone()}</code>
            </span>
        </span>
        <span class=style::state_cell>
            <StatusTag standing />
        </span>
        <span>
            <AutoToggle app=id device />
        </span>
    }
}
