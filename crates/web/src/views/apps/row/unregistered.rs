use android::apps::Package;
use leptos::prelude::*;

use super::super::icon::AppIcon;
use super::super::labels::Labels;
use super::super::style;
use crate::ui::loadable::Loadable;

/// It has no state against the registry and no Auto-update, so their cells
/// stay empty.
#[allow(unreachable_pub)]
#[component]
pub(super) fn Unregistered(
    package: Package,
    labels: Labels,
    running: Signal<bool>,
) -> impl IntoView {
    let id = package.id.clone();
    // A memo, so the row is drawn again only when its own label arrives and
    // not for every batch.
    let label = Memo::new(move |_| labels.of(&package));
    let name = Memo::new(move |_| {
        label.with(|label| match label {
            Loadable::Ready(label) => label.name.clone(),
            Loadable::Loading | Loadable::Failed(_) => None,
        })
    });
    let version = move || {
        label.with(|label| match label {
            Loadable::Ready(label) => label.version.clone(),
            Loadable::Loading | Loadable::Failed(_) => None,
        })
    };
    let drawn = Signal::derive(move || label.get().map(|label| label.icon));
    let title = {
        let id = id.clone();

        move || name.get().unwrap_or_else(|| id.clone())
    };
    let named = id.clone();

    view! {
        <AppIcon id=id drawn running />
        <span class=style::names>
            <span class=style::title>
                <strong>{title}</strong>
                {move || {
                    version().map(|version| view! { <span class=style::version>{version}</span> })
                }}
            </span>
            // An app without a name of its own already shows its id.
            {move || {
                name.with(Option::is_some)
                    .then(|| {
                        view! {
                            <span class=style::id>
                                <code>{named.clone()}</code>
                            </span>
                        }
                    })
            }}
        </span>
        <span class=style::state_cell></span>
        <span></span>
    }
}
