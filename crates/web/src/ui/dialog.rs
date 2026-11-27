//! A dialog opens as it is built, so the view shows it with `<Show>` or a
//! match, and takes it out again when `on_close` says it closed.

use leptos::prelude::*;
use leptos::{ev, html};
use stylance::classes;
use wasm_bindgen::JsCast;

use crate::ui;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "dialog.module.scss"
);

/// A press that starts inside and is released outside does not count, so that
/// text in the dialog can still be selected by dragging.
fn close_beside(event: &ev::MouseEvent) {
    let Some(dialog) = event
        .current_target()
        .filter(|dialog| event.target().as_ref() == Some(dialog))
        .and_then(|dialog| dialog.dyn_into::<web_sys::HtmlDialogElement>().ok())
    else {
        return;
    };

    // A press on the dialog's own padding also targets the dialog, so the
    // position decides.
    let within = dialog.get_bounding_client_rect();
    let x = event.client_x();
    let y = event.client_y();

    if x < within.left() || x > within.right() || y < within.top() || y > within.bottom() {
        dialog.close();
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Dialog(
    #[prop(into)] title: String,
    /// Without it the dialog can't be closed.
    #[prop(optional, into)]
    on_close: Option<Callback<()>>,
    #[prop(optional)] alert: bool,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    let dialog = NodeRef::<html::Dialog>::new();
    let called = title.clone();

    // A dialog only opens as a modal once it is in the page, which it is
    // by the time effects run.
    Effect::new(move |_| {
        if let Some(dialog) = dialog.get() {
            let _ = dialog.show_modal();
        }
    });

    view! {
        <dialog
            node_ref=dialog
            class=classes!(style::dialog, class)
            role=alert.then_some("alertdialog")
            aria-label=called
            on:close=move |_| {
                if let Some(on_close) = on_close {
                    on_close.run(());
                }
            }
            on:cancel=move |event: ev::Event| {
                // Escape would close it.
                if on_close.is_none() {
                    event.prevent_default();
                }
            }
            on:mousedown=move |event| {
                if on_close.is_some() {
                    close_beside(&event);
                }
            }
        >
            <h2 class=ui::style::title>{title}</h2>
            {children()}
        </dialog>
    }
}
