use std::collections::HashSet;

use leptos::prelude::*;
use stylance::classes;

use super::folder::Doomed;
use super::format::sign;
use super::{Entry, download, style};
use crate::app::{Global, Icon, Page, Picked, icons};
use crate::link;
use crate::ui::action::use_action;
use crate::ui::button::{self, IconButton};
use crate::ui::menu::MenuOf;
use crate::ui::pick::{self, Picking, Tick};
use crate::util::size;

/// The largest file, in bytes, that opens in the editor. A larger one is
/// saved to this computer instead.
const CHANGED_HERE: u64 = 512 * 1024;

#[allow(unreachable_pub)]
#[component]
pub(super) fn FileRow(
    entry: Entry,
    at: String,
    picked: RwSignal<Vec<String>>,
    picking: Picking,
    menu: MenuOf<Entry>,
    saved: RwSignal<HashSet<String>>,
    editing: RwSignal<Option<(String, String)>>,
    on_delete: Callback<Doomed>,
) -> impl IntoView {
    let Global { link, router, .. } = expect_context::<Global>();
    let device = expect_context::<Picked>().device;
    let folder = entry.folder;
    let bytes = entry.size;
    let at = StoredValue::new(at);
    let name = StoredValue::new(entry.name.clone());
    let opened = StoredValue::new(entry.clone());
    let is_picked = pick::is_picked(picked, at.get_value());
    let is_saved =
        Signal::derive(move || saved.with(|saved| at.with_value(|at| saved.contains(at))));
    let label = move || {
        if is_saved.get() {
            "Downloaded"
        } else {
            "Download"
        }
    };

    let (saving, save) = use_action(Some("files"), move |()| {
        download(
            device.get_untracked(),
            link,
            saved,
            vec![(at.get_value(), folder)],
        )
    });
    let (_, read) = use_action(Some("files"), move |()| {
        let device = device.get_untracked();
        let at = at.get_value();
        let name = name.get_value();

        async move {
            match String::from_utf8(device.read(&at).await?) {
                // Nothing reads it once the view closed.
                Ok(text) => {
                    editing.try_set(Some((at, text)));
                }
                Err(file) => link::save(&name, file.as_bytes()),
            }

            Ok(None::<String>)
        }
    });
    let open = move || {
        if folder {
            router.go(Page::Files {
                folder: at.get_value(),
            });
        } else if bytes > CHANGED_HERE {
            save.run(());
        } else {
            read.run(());
        }
    };

    let title = format!("Delete {}?", entry.name);
    let body = if folder {
        "The folder and all that is in it are deleted from the device."
    } else {
        "It is deleted from the device."
    };

    view! {
        <div
            class=classes!(style::file, style::item)
            class=(style::picked, is_picked)
            data-pick=at.get_value()
            on:contextmenu=move |event| menu.at_pointer(&event, opened.get_value())
        >
            <Tick
                picked=is_picked
                label=format!("Pick {}", entry.name)
                class=style::tick
                on:click=move |_| at.with_value(|at| pick::toggle(picked, at))
            />
            <button
                type="button"
                class=classes!(style::name, folder.then_some(style::folder))
                title="Double-click to open"
                aria-pressed=move || is_picked.get().to_string()
                on:click=move |_| {
                    // The click that ends a drag selection is ignored.
                    if !picking.swept() {
                        at.with_value(|at| pick::toggle(picked, at));
                    }
                }
                on:dblclick=move |_| open()
                on:keydown=move |event| {
                    if event.key() == "Enter" {
                        event.prevent_default();
                        open();
                    }
                }
            >
                <Icon paths=if folder { icons::FOLDER } else { sign(&entry.name) } />
                <span>{entry.name.clone()}</span>
            </button>
            <code>{(!folder).then(|| size(bytes))}</code>
            <code>{entry.changed}</code>
            <span class=style::acts>
                <button
                    type="button"
                    class=classes!(button::style::icon, button::style::small)
                    class=(style::done, is_saved)
                    title=label
                    aria-label=label
                    disabled=move || saving.get()
                    on:click=move |_| save.run(())
                >
                    {move || view! { <Icon paths=if is_saved.get() { icons::CHECK } else { icons::DOWNLOAD } /> }}
                </button>
                <IconButton
                    paths=icons::BIN
                    label="Delete from the device"
                    small=true
                    on:click=move |_| on_delete.run((vec![at.get_value()], title.clone(), body))
                />
            </span>
        </div>
    }
}
