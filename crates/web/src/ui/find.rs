use leptos::html;
use leptos::prelude::*;

use crate::app::icons;
use crate::ui::button::IconButton;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "find.module.scss"
);

/// The text of the box is a signal of the view's own, since it stays while
/// the bar is closed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Finding<O> {
    Closed,
    /// None while nothing was searched for.
    Open(Option<O>),
}

impl<O> Finding<O> {
    pub(crate) fn is_open(&self) -> bool {
        matches!(self, Finding::Open(_))
    }

    pub(crate) fn open(&mut self) {
        if !self.is_open() {
            *self = Finding::Open(None);
        }
    }

    pub(crate) fn outcome(self) -> Option<O> {
        match self {
            Finding::Open(outcome) => outcome,
            Finding::Closed => None,
        }
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Find(
    label: &'static str,
    sought: RwSignal<String>,
    #[prop(into)] count: Signal<Option<String>>,
    #[prop(optional, into)] none: Signal<bool>,
    #[prop(optional)] as_typed: bool,
    /// True steps backwards.
    #[prop(into)]
    find: Callback<bool>,
    #[prop(into)] close: Callback<()>,
) -> impl IntoView {
    let field = NodeRef::<html::Input>::new();

    // The last search is selected, so that typing replaces it.
    Effect::new(move |_| {
        if let Some(field) = field.get() {
            let _ = field.focus();

            field.select();
        }
    });

    view! {
        <div class=style::find class=(style::none, move || none.get()) role="search">
            <input
                node_ref=field
                aria-label=label
                placeholder="Find"
                autocomplete="off"
                spellcheck="false"
                bind:value=sought
                on:input=move |_| {
                    if as_typed {
                        find.run(false);
                    }
                }
                on:keydown=move |event| match event.key().as_str() {
                    "Enter" => {
                        event.prevent_default();
                        find.run(event.shift_key());
                    }
                    "Escape" => {
                        // Keeps the panel around the terminal from closing too.
                        event.prevent_default();
                        close.run(());
                    }
                    _ => {}
                }
            />
            <span class=style::count role="status">{count}</span>
            <IconButton
                paths=icons::UP
                label="Previous · Shift+Enter"
                class=style::step
                on:click=move |_| find.run(true)
            />
            <IconButton
                paths=icons::DOWN
                label="Next · Enter"
                class=style::step
                on:click=move |_| find.run(false)
            />
            <IconButton
                paths=icons::CLOSE
                label="Close · Esc"
                class=style::step
                on:click=move |_| close.run(())
            />
        </div>
    }
}
