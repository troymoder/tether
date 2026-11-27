use std::collections::HashSet;

use leptos::prelude::*;
use stylance::classes;

use super::folder::Doomed;
use super::format::within;
use super::row::FileRow;
use super::search::{Match, SearchResults};
use super::{Entry, style};
use crate::ui::loadable::{Loadable, Status};
use crate::ui::menu::MenuOf;
use crate::ui::pick::{PickedBar, Picking};
use crate::util::counted;

/// None while nothing is sought and while a search is on its way, when the
/// folder's own entries stay.
pub(super) type Found = Option<Loadable<Vec<Match>>>;

#[allow(unreachable_pub)]
#[component]
pub(super) fn FileList(
    folder: String,
    listed: Signal<Loadable<Vec<Entry>>>,
    found: Memo<Found>,
    missing: Signal<bool>,
    picked: RwSignal<Vec<String>>,
    picking: Picking,
    menu: MenuOf<Entry>,
    saved: RwSignal<HashSet<String>>,
    editing: RwSignal<Option<(String, String)>>,
    on_reach: Callback<(String, bool)>,
    on_save: Callback<Vec<String>>,
    on_delete: Callback<Doomed>,
) -> impl IntoView {
    let folder = StoredValue::new(folder);
    let shown = Signal::derive(move || {
        listed.with(|listed| match listed {
            Loadable::Ready(entries) => entries
                .iter()
                .map(|entry| folder.with_value(|folder| within(folder, &entry.name)))
                .collect::<Vec<String>>(),
            Loadable::Loading | Loadable::Failed(_) => Vec::new(),
        })
    });
    let delete_picked = move |()| {
        let paths = picked.get_untracked();
        let title = format!("Delete {}?", counted(paths.len()));

        on_delete.run((
            paths,
            title,
            "They are deleted from the device, folders with all that is in them.",
        ));
    };

    view! {
        <div class=style::frame>
            <div class=style::list>
                <div data-picks>
                    <div class=classes!(style::file, style::head)>
                        <span></span>
                        <span>"Name"</span>
                        <span>"Size"</span>
                        <span>"Changed"</span>
                        <span></span>
                    </div>
                    <Show when=move || missing.get()>
                        <p class=classes!(
                            style::nothing, style::error
                        )>"No such folder or file."</p>
                    </Show>
                    {move || match found.get() {
                        Some(Loadable::Ready(matches)) => {
                            view! { <SearchResults folder=folder.get_value() matches on_reach /> }
                                .into_any()
                        }
                        Some(Loadable::Failed(why)) => {
                            view! { <p class=classes!(style::nothing, style::error)>{why}</p> }
                                .into_any()
                        }
                        Some(Loadable::Loading) | None => {
                            view! {
                                <Status
                                    of=listed
                                    empty=(Vec::is_empty, "Nothing in this folder.")
                                    class=style::nothing
                                    failed=style::error
                                    let:entries
                                >
                                    // Keyed by every field, so a listing loaded again
                                    // rebuilds the row of an entry that changed. A row
                                    // reused for another entry keeps the first entry's
                                    // handlers.
                                    <For
                                        each=move || entries.clone()
                                        key=|entry| {
                                            (
                                                entry.name.clone(),
                                                entry.folder,
                                                entry.size,
                                                entry.changed.clone(),
                                            )
                                        }
                                        let:entry
                                    >
                                        <FileRow
                                            at=folder.with_value(|folder| within(folder, &entry.name))
                                            entry
                                            picked
                                            picking
                                            menu
                                            saved
                                            editing
                                            on_delete
                                        />
                                    </For>
                                </Status>
                            }
                                .into_any()
                        }
                    }}
                </div>
            </div>
            <PickedBar
                picked
                shown
                all="Pick all"
                on_download=move |()| on_save.run(picked.get_untracked())
                on_delete=delete_picked
            />
        </div>
    }
}
