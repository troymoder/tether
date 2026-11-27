use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use stylance::classes;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, Node};

use crate::ui::button::style::lit as lit_class;
use crate::ui::floating::{Anchor, close_on_scroll, use_floating};

mod radios;

pub(crate) use radios::Radios;

// Public so content inside a box can use the heading, rule and rest classes.
stylance::import_style!(#[allow(dead_code, unreachable_pub)] pub style, "popup.module.scss");

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Popup(
    #[prop(into)] button: ViewFn,
    #[prop(optional, into)] button_class: String,
    #[prop(optional)] id: Option<&'static str>,
    #[prop(optional, into)] button_label: Option<Signal<String>>,
    label: &'static str,
    #[prop(optional)] class: &'static str,
    #[prop(optional, into)] lit: Signal<bool>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] right: bool,
    #[prop(optional)] as_wide: bool,
    /// The open state, passed in when the content needs to close the box.
    #[prop(optional)]
    open: Option<RwSignal<bool>>,
    children: ChildrenFn,
) -> impl IntoView {
    let open = open.unwrap_or_else(|| RwSignal::new(false));
    let whole = NodeRef::<html::Span>::new();
    let trigger = NodeRef::<html::Button>::new();
    let boxed = NodeRef::<html::Div>::new();
    let floating = use_floating(boxed);

    let close = move || {
        open.set(false);

        // A disabled button, or one in a disabled fieldset, cannot take the
        // focus.
        if let Some(trigger) = trigger.get_untracked()
            && !trigger.matches(":disabled").unwrap_or(false)
        {
            let _ = trigger.focus();
        }
    };

    let show = move || {
        open.set(true);
        floating.place(
            move || {
                let trigger = trigger.try_get_untracked().flatten()?;

                Some(Anchor::button(&trigger, right, as_wide))
            },
            |boxed| {
                let chosen = boxed
                    .query_selector("[aria-checked=\"true\"]:not(:disabled)")
                    .ok()
                    .flatten()
                    .and_then(|chosen| chosen.dyn_into::<HtmlElement>().ok());
                let _ = chosen.unwrap_or_else(|| boxed.into()).focus();
            },
        );
    };

    // Rendered again while open, as when its parent is, so it is placed
    // again.
    if open.get_untracked() {
        show();
    }

    // Scrolling what the button is in would leave the box behind, so the box
    // closes. Other scrolls, such as the box's own or a log following its
    // end, leave it open.
    close_on_scroll(
        move |under| {
            open.try_get_untracked().unwrap_or(false)
                && trigger
                    .try_get_untracked()
                    .flatten()
                    .is_some_and(|trigger| under.is_none_or(|under| under.contains(Some(&trigger))))
        },
        move || {
            open.try_set(false);
        },
    );

    view! {
        <span
            node_ref=whole
            class=style::anchor
            // A press anywhere else moves the focus out and closes the box.
            on:focusout=move |event| {
                let Some(whole) = whole.try_get_untracked().flatten() else {
                    return;
                };

                let to = event.related_target().and_then(|to| to.dyn_into::<Node>().ok());

                if !to.is_some_and(|to| whole.contains(Some(&to))) {
                    open.try_set(false);
                }
            }
            on:keydown=move |event: ev::KeyboardEvent| {
                // A dropdown inside closes its own list first and stops the
                // key before it gets here.
                if event.key() == "Escape" && open.get_untracked() {
                    event.prevent_default();
                    event.stop_propagation();
                    close();
                }
            }
        >
            <button
                node_ref=trigger
                type="button"
                id=id
                class=move || classes!(&button_class, lit.get().then_some(lit_class))
                title=move || button_label.map(|label| label.get())
                aria-label=move || button_label.map(|label| label.get())
                aria-haspopup="dialog"
                aria-expanded=move || open.get().to_string()
                disabled=move || disabled.get()
                // Safari and Firefox on macOS do not focus a pressed button.
                // The focus would leave the box and close it, and the click
                // would open it again, so the button is focused here.
                on:mousedown=move |event| {
                    event.prevent_default();

                    if let Some(trigger) = trigger.get_untracked() {
                        let _ = trigger.focus();
                    }
                }
                on:click=move |_| if open.get_untracked() { close() } else { show() }
            >
                {button.run()}
            </button>
            <Show when=move || open.get()>
                <div
                    node_ref=boxed
                    class=classes!(style::popup, class)
                    role="dialog"
                    aria-label=label
                    popover="manual"
                    // Keeps the focus in the box when something unfocusable
                    // inside it is pressed, such as a disabled choice.
                    tabindex="-1"
                    style=move || floating.style()
                >
                    {children()}
                </div>
            </Show>
        </span>
    }
}
