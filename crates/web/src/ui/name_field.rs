//! The field sits over a hidden copy of its text, so that it grows and
//! shrinks as it is typed.

use leptos::prelude::*;
use leptos::{ev, html};
use stylance::classes;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "name_field.module.scss"
);

/// It only stands while the name is still the one it was typed over, so that
/// a name that changes under it shows.
struct Draft {
    text: String,
    over: String,
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn NameField(
    #[prop(into)] name: Signal<String>,
    #[prop(into)] renamed: Callback<String>,
    /// Given, the field renames in place of the name's plain text, and this
    /// is called when the edit ends, kept or not.
    #[prop(optional, into)]
    done: Option<Callback<()>>,
) -> impl IntoView {
    let field = NodeRef::<html::Input>::new();
    let draft = RwSignal::new(None::<Draft>);
    let shown = move || {
        let now = name.get();

        draft.with(|draft| match draft {
            Some(draft) if draft.over == now => draft.text.clone(),
            _ => now,
        })
    };
    let type_over = move |text: String| {
        draft.set(Some(Draft {
            text,
            over: name.get_untracked(),
        }));
    };

    if done.is_some() {
        field.on_load(|input| {
            // Focused once it is in the page, which it is not yet on load.
            request_animation_frame(move || {
                let _ = input.focus();
                input.select();
            });
        });
    }

    let keep = move || {
        let typed = untrack(shown).trim().to_owned();
        let now = name.get_untracked();

        // A kept edit stays shown until the name itself changes, so the
        // old name does not come back for the moment the rename takes.
        if !typed.is_empty() && typed != now {
            type_over(typed.clone());
            renamed.run(typed);
        } else {
            draft.set(None);
        }

        if let Some(done) = done {
            done.run(());
        }
    };

    // A press on the padding around the text counts as a press in the field
    // and puts the caret at the nearer end.
    let padding = move |event: ev::MouseEvent| {
        let Some(input) = field.get_untracked() else {
            return;
        };

        if event.target().as_ref() == Some(input.as_ref()) {
            return;
        }

        event.prevent_default();

        let left = event.client_x() < input.get_bounding_client_rect().left();
        let at = if left {
            0
        } else {
            input.value().encode_utf16().count() as u32
        };

        let _ = input.focus();
        let _ = input.set_selection_range(at, at);
    };

    view! {
        <span
            class=classes!(style::field, done.is_some().then_some(style::inline))
            on:mousedown=padding
        >
            <span class=style::ghost aria-hidden="true">
                {move || {
                    let shown = shown();

                    if shown.is_empty() { " ".to_owned() } else { shown }
                }}
            </span>
            <input
                node_ref=field
                aria-label="Name"
                prop:value=shown
                on:input=move |event| type_over(event_target_value(&event))
                on:keydown=move |event: ev::KeyboardEvent| {
                    // Keys typed here are not for the card or row it sits on,
                    // and Escape closes nothing around it.
                    event.stop_propagation();

                    let Some(field) = field.get_untracked() else {
                        return;
                    };

                    match event.key().as_str() {
                        // The blur handler keeps the edit.
                        "Enter" => {
                            event.prevent_default();
                            let _ = field.blur();
                        }
                        "Escape" => {
                            event.prevent_default();
                            draft.set(None);
                            let _ = field.blur();
                        }
                        _ => {}
                    }
                }
                on:blur=move |_| keep()
                // Keeps the click from reaching the row the field sits on.
                on:click=|event| event.stop_propagation()
                on:dblclick=|event| event.stop_propagation()
            />
        </span>
    }
}
