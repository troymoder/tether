use std::collections::HashSet;

use leptos::ev;
use leptos::prelude::*;

use super::editor::Editor;
use super::format::{above, within};
use super::list::FileList;
use super::toolbar::Toolbar;
use super::{Entry, Typed, download, style};
use crate::app::{Global, Page, Picked};
use crate::ui::action::use_action;
use crate::ui::dropdown::Choice;
use crate::ui::load::use_load;
use crate::ui::loadable::Loadable;
use crate::ui::menu::use_menu;
use crate::ui::pane::Pane;
use crate::ui::pick::{Picking, selection_menu};
use crate::util::counted;

/// The paths to delete, then the title and the body of the question that is
/// asked first.
pub(super) type Doomed = (Vec<String>, String, &'static str);

#[allow(unreachable_pub)]
#[component]
pub(super) fn Folder(
    at: String,
    start: Option<String>,
    to_pick: StoredValue<Option<String>>,
    changed: Trigger,
    editing: RwSignal<Option<(String, String)>>,
    saved: RwSignal<HashSet<String>>,
    places: Signal<Loadable<Vec<Choice<String>>>>,
    upload: Callback<web_sys::File>,
) -> impl IntoView {
    let Global { link, router, ui } = expect_context::<Global>();
    let device = expect_context::<Picked>().device;
    let folder = StoredValue::new(at);
    let picking = Picking::new();
    let picked = RwSignal::new(start.into_iter().collect::<Vec<String>>());
    let typed = Typed::new();
    let menu = use_menu::<Entry>();
    let listed = use_load(
        move || changed.track(),
        move |()| {
            let device = device.get();
            let at = folder.get_value();

            async move { device.list(&at).await }
        },
    );
    let searched = typed.use_found(move |wanted: String| {
        let device = device.get_untracked();
        let at = folder.get_value();

        async move { device.find(&at, &wanted).await }
    });
    // The folder's own entries stay until the matches are here, so a search
    // on its way counts as none.
    let found = Memo::new(move |_| searched.get().filter(|found| *found != Loadable::Loading));
    let reach = Callback::new(move |(at, is_folder): (String, bool)| {
        if is_folder {
            router.go(Page::Files { folder: at });
            return;
        }

        let around = above(&at);

        if folder.with_value(|folder| *folder == around) {
            // The open folder is not built again, so the file is picked
            // here and the search that found it is cleared.
            typed.clear();
            picked.set(vec![at]);
        } else {
            to_pick.set_value(Some(at));
            router.go(Page::Files { folder: around });
        }
    });

    let go_to = typed.use_path(move |path: String| {
        let at = if path.starts_with('/') {
            path
        } else {
            folder.with_value(|folder| within(folder, &path))
        };
        let at = format!("/{}", at.trim_matches('/'));
        let device = device.get_untracked();

        async move {
            match device.entry_kind(&at).await {
                Ok(Some(is_folder)) => {
                    // Another folder may have opened by now, and then
                    // nothing of this one is left to tell.
                    reach.try_run((at, is_folder));

                    true
                }
                _ => false,
            }
        }
    });

    let (_, fetch) = use_action(Some("files"), move |paths: Vec<(String, bool)>| {
        download(device.get_untracked(), link, saved, paths)
    });
    // A folder is saved as a tar, so each path is looked up in the listing
    // first.
    let save = Callback::new(move |paths: Vec<String>| {
        let folders: Vec<String> = listed.with_untracked(|listed| match listed {
            Loadable::Ready(entries) => entries
                .iter()
                .filter(|entry| entry.folder)
                .map(|entry| folder.with_value(|folder| within(folder, &entry.name)))
                .collect(),
            Loadable::Loading | Loadable::Failed(_) => Vec::new(),
        });
        let paths = paths
            .into_iter()
            .map(|at| {
                let is_folder = folders.contains(&at);

                (at, is_folder)
            })
            .collect();

        fetch.run(paths);
    });

    let (_, remove) = use_action(Some("files"), move |paths: Vec<String>| {
        let device = device.get_untracked();

        async move {
            let mut said = Ok(format!("Deleted {}", counted(paths.len())));

            for at in &paths {
                if let Err(error) = device.delete(at).await {
                    said = Err(error);
                    break;
                }
            }

            // The listing loads again whatever was deleted before it failed.
            picked.try_update(|picked| picked.retain(|at| !paths.contains(at)));
            changed.notify();

            said
        }
    });
    let delete = Callback::new(move |(paths, title, body): Doomed| {
        ui.confirm_then(title, body, "Delete", async move {
            remove.try_run(paths);
        });
    });

    // The memo holds only the path. The editor reads the text itself, so
    // typing builds nothing again.
    let open =
        Memo::new(move |_| editing.with(|editing| editing.as_ref().map(|(at, _)| at.clone())));
    let count = move || {
        listed.with(|listed| match listed {
            Loadable::Ready(entries) => Some(counted(entries.len())),
            Loadable::Loading | Loadable::Failed(_) => None,
        })
    };

    view! {
        <Pane
            title="Files"
            class=style::files
            on:pointerdown=move |event: ev::PointerEvent| picking.start(&event, style::swept, picked)
        >
            <Toolbar path=folder.get_value() sought=typed.text places on_path=go_to upload />
            {move || match open.get() {
                Some(at) => view! { <Editor at editing changed /> }.into_any(),
                None => {
                    view! {
                        <FileList
                            folder=folder.get_value()
                            listed
                            found
                            missing=Signal::derive(move || typed.missing())
                            picked
                            picking
                            menu
                            saved
                            editing
                            on_reach=reach
                            on_save=save
                            on_delete=delete
                        />
                    }
                        .into_any()
                }
            }}
            <div class=style::under>
                <span>{count}</span>
                <span>"Drop files to upload."</span>
            </div>
            {menu
                .view(
                    false,
                    move |entry: &Entry| {
                        let path = folder.with_value(|folder| within(folder, &entry.name));

                        Some(selection_menu(picked, path, entry.name.clone(), save, None))
                    },
                )}
        </Pane>
    }
}
