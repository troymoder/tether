use leptos::prelude::*;
use stylance::classes;

use crate::ui;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Dot {
    /// Grey: known and not reached.
    Grey,
    /// A grey outline: off, offline or not there.
    Ring,
    /// Blue: found and there to connect to.
    Found,
    /// A blue outline: found, and asking to be paired first.
    FoundRing,
    /// Amber: starting, stopping or connecting.
    Starting,
    /// Amber: something to look at, such as an update that is ready.
    Warn,
    /// Red: failed.
    Bad,
    /// Green: running or connected.
    On,
}

impl Dot {
    pub(crate) fn class(self) -> String {
        use ui::style::{bad, dot, found, on, ring, starting, warn};

        match self {
            Dot::Grey => dot.to_owned(),
            Dot::Ring => classes!(dot, ring),
            Dot::Found => classes!(dot, found),
            Dot::FoundRing => classes!(dot, ring, found),
            Dot::Starting => classes!(dot, starting),
            Dot::Warn => classes!(dot, warn),
            Dot::Bad => classes!(dot, bad),
            Dot::On => classes!(dot, on),
        }
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Tag(
    #[prop(optional, into)] tone: MaybeProp<Dot>,
    #[prop(optional, into)] title: MaybeProp<String>,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <span class=classes!(ui::style::tag, class) title=move || title.get()>
            {move || {
                tone.get().map(|tone| view! { <span class=tone.class() aria-hidden="true"></span> })
            }}
            {children()}
        </span>
    }
}
