//! Unlike a popup's box (popup.rs), this one is laid out by the view, which
//! gives it a class that places it.

use leptos::prelude::*;
use stylance::classes;
use wasm_bindgen::JsCast;

use crate::link::heard::Heard;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "popbox.module.scss"
);

/// A box can be open by more than a flag of its own, as one of several lists
/// of which one is open at a time. The flag a view's button sets turns into
/// one by itself.
#[derive(Clone, Copy)]
pub(crate) struct Open {
    is: Signal<bool>,
    close: Callback<()>,
}

impl Open {
    pub(crate) fn new(is: Signal<bool>, close: Callback<()>) -> Self {
        Open { is, close }
    }
}

impl From<RwSignal<bool>> for Open {
    fn from(open: RwSignal<bool>) -> Self {
        Open {
            is: open.into(),
            close: Callback::new(move |()| open.set(false)),
        }
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn PopBox(
    #[prop(into)] open: Open,
    #[prop(into)] label: String,
    class: &'static str,
    children: ChildrenFn,
) -> impl IntoView {
    let mut heard = Heard::new(&document());

    // Esc closes the box wherever the focus is. It is heard on the
    // document, after an open list or a typed path inside the box, which
    // stops the key. The page's own handler on the window (app.rs) comes
    // later and skips a handled key, so the panel behind the box stays open.
    heard.on("keydown", move |event| {
        let escape = event
            .dyn_ref::<web_sys::KeyboardEvent>()
            .is_some_and(|event| event.key() == "Escape");

        if escape && !event.default_prevented() && open.is.try_get_untracked() == Some(true) {
            event.prevent_default();
            open.close.run(());
        }
    });

    // Dropped with the component, which takes the listener off.
    let _heard = StoredValue::new_local(heard);

    view! {
        <Show when=move || open.is.get()>
            <button
                type="button"
                class=style::away
                aria-label="Close"
                on:click=move |_| open.close.run(())
            ></button>
            <div role="dialog" aria-label=label.clone() class=classes!(style::boxed, class)>
                {children()}
            </div>
        </Show>
    }
}
