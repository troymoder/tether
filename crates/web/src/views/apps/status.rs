use leptos::prelude::*;

use super::installed::Standing;
use super::style;
use crate::ui::loadable::Loadable;
use crate::ui::tag::{Dot, Tag};

#[allow(unreachable_pub)]
#[component]
pub(crate) fn StatusTag(#[prop(into)] standing: Signal<Loadable<Standing>>) -> impl IntoView {
    let words = move || match standing.get() {
        Loadable::Loading => "Looking…",
        Loadable::Failed(_) => "No answer",
        Loadable::Ready(standing) => match standing {
            Standing::Absent => "Not installed",
            Standing::Unfit => "No build for it",
            Standing::Current => "Up to date",
            Standing::Behind => "Update ready",
            Standing::Installing => "Installing…",
            Standing::Failed(_) => "Install failed",
        },
    };
    let tone = Signal::derive(move || match standing.get() {
        Loadable::Ready(Standing::Current) => Dot::On,
        Loadable::Ready(Standing::Behind) => Dot::Warn,
        Loadable::Ready(Standing::Installing) => Dot::Starting,
        Loadable::Ready(Standing::Failed(_)) => Dot::Bad,
        _ => Dot::Ring,
    });
    let title = Signal::derive(move || match standing.get() {
        Loadable::Failed(reason) | Loadable::Ready(Standing::Failed(reason)) => reason,
        _ => words().to_owned(),
    });

    view! {
        <Tag tone title>
            <span class=style::words>{words}</span>
        </Tag>
    }
}
