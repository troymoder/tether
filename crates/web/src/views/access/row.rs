use leptos::ev;
use leptos::prelude::*;
use protocol::Request;
use wasm_bindgen::JsCast;

use super::{Key, Rows, style};
use crate::app::Global;
use crate::ui::renamable::Renamable;

fn step(up: bool) {
    let Ok(listed) = document().query_selector_all("[data-row]") else {
        return;
    };

    let rows: Vec<web_sys::HtmlElement> = (0..listed.length())
        .filter_map(|at| listed.item(at)?.dyn_into().ok())
        .collect();
    let focused = document().active_element();
    let Some(at) = rows.iter().position(|row| focused.as_ref() == Some(&**row)) else {
        return;
    };

    let to = if up { at.checked_sub(1) } else { Some(at + 1) };

    if let Some(row) = to.and_then(|to| rows.get(to)) {
        let _ = row.focus();
    }
}

/// Focusing a row selects it, so the arrow keys move the selection by
/// moving the focus.
#[allow(unreachable_pub)]
#[component]
pub(super) fn Selectable(rows: Rows, of: Key, children: Children) -> impl IntoView {
    let key = StoredValue::new(of);
    let chosen = Memo::new(move |_| {
        key.with_value(|key| rows.chosen.with(|chosen| chosen.as_ref() == Some(key)))
    });

    view! {
        <div
            class=style::row
            class=(style::chosen, move || chosen.get())
            tabindex="0"
            data-row=""
            aria-current=move || chosen.get().then_some("true")
            on:focus=move |_| rows.chosen.set(Some(key.get_value()))
            on:click=move |_| rows.chosen.set(Some(key.get_value()))
            on:keydown=move |event: ev::KeyboardEvent| {
                match event.key().as_str() {
                    "ArrowUp" => step(true),
                    "ArrowDown" => step(false),
                    "F2" => rows.renaming.start(key.get_value()),
                    "Delete" | "Backspace" => rows.remove.run(key.get_value()),
                    _ => return,
                }

                event.prevent_default();
            }
            on:contextmenu=move |event| {
                rows.chosen.set(Some(key.get_value()));
                rows.menu.at_pointer(&event, key.get_value());
            }
        >
            {children()}
        </div>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Called(rows: Rows, of: Key, #[prop(into)] name: Signal<String>) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let primary = untrack(|| of.primary(link));
    let renamed = {
        let of = of.clone();

        move |name: String| match &of {
            Key::Client(print) => link.ask(Request::Rename {
                print: print.clone(),
                name,
            }),
            Key::Instance(_) if primary => link.ask(Request::RenameInstance { name }),
            Key::Instance(_) => link.unimplemented("rename instance"),
        }
    };

    view! {
        <Renamable renaming=rows.renaming of name renamed>
            <span>{name}</span>
        </Renamable>
    }
}
