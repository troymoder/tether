use std::rc::Rc;

use leptos::prelude::*;

use super::format::within;
use super::style;
use crate::app::{Global, Page, Picked, icons};
use crate::ui::button::IconButton;
use crate::ui::dropdown::{Choice, Dropdown};
use crate::ui::file_pick::{FilePick, FilePicker};
use crate::ui::loadable::{Loadable, Status};
use crate::ui::pathbar::{self, PathBar};

#[allow(unreachable_pub)]
#[component]
pub(super) fn Toolbar(
    path: String,
    sought: RwSignal<String>,
    places: Signal<Loadable<Vec<Choice<String>>>>,
    on_path: Callback<String>,
    upload: Callback<web_sys::File>,
) -> impl IntoView {
    let router = expect_context::<Global>().router;
    let device = expect_context::<Picked>().device;
    let chooser = FilePicker::new();
    let open = StoredValue::new(path.clone());
    let go = move |folder: String| router.go(Page::Files { folder });
    let folders: pathbar::Folders = Rc::new(move |of: String| {
        let device = device.get_untracked();

        Box::pin(async move {
            let listed = device.list(&of).await?;

            Ok(listed
                .into_iter()
                .filter(|entry| entry.folder)
                .map(|entry| {
                    let to = within(&of, &entry.name);

                    (entry.name, to)
                })
                .collect())
        })
    });
    // Stands in place of the storage dropdown while it is asked for or when
    // asking failed, so the row holds still.
    let no_storage = |why: String| {
        view! { <IconButton paths=icons::STORAGE label=why framed=true disabled=true /> }.into_any()
    };

    view! {
        <div class=style::path>
            <PathBar path folders on_go=go find=sought on_path />
            <Status of=places loading="Looking for storage…" line=no_storage let:choices>
                <Dropdown
                    class=style::storage
                    label="Which storage"
                    placeholder="Storage"
                    icon=icons::STORAGE
                    choices
                    chosen=Signal::derive(move || Some(open.get_value()))
                    on_choose=go
                />
            </Status>
            <FilePick picker=chooser multiple=true label="Files for this folder" on_file=upload />
            <IconButton
                paths=icons::UPLOAD
                label="Upload files to this folder"
                framed=true
                on:click=move |_| chooser.open()
            />
        </div>
    }
}
