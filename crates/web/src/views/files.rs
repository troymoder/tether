//! [`Files`] holds what outlasts a folder: the file open in the editor, the
//! ticks of what was saved, and the upload. One [`Folder`] is built under it
//! for each folder the address names, so a folder's listing, search and
//! picked rows start fresh with it.

use std::collections::HashSet;
use std::rc::Rc;

use leptos::prelude::*;

use crate::app::{Global, Page, Picked};
use crate::device::Device;
use crate::link::{self, Link};
use crate::ui::action::use_action_many;
use crate::ui::dropdown::Choice;
use crate::ui::load::use_load;
use crate::util::counted;

mod device;
mod editor;
mod folder;
mod format;
mod list;
mod row;
mod search;
mod toolbar;
mod typed;

pub(crate) use device::{Entry, Files};
use folder::Folder;
use format::{saved_as, within};
pub(crate) use typed::Typed;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "files.module.scss"
);

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Files() -> impl IntoView {
    let Global { link, router, ui } = expect_context::<Global>();
    let device = expect_context::<Picked>().device;

    // The apps' folders are named after the registered apps.
    link.follow(|| Some(protocol::Topic::Apps));

    let path = router.held(|page| match page {
        Page::Files { folder } => Some(folder),
        _ => None,
    });
    let changed = Trigger::new();
    let editing = RwSignal::new(None::<(String, String)>);
    let saved = RwSignal::new(HashSet::<String>::new());
    // A file reached by search or by typed path, which the folder that
    // opens next starts with picked.
    let to_pick = StoredValue::new(None::<String>);
    // The storage is asked for here, since a folder is built again each time
    // another opens.
    let places = use_load(
        || (),
        move |()| {
            let device = device.get();

            async move {
                let places = device.places().await?;
                let choices: Vec<Choice<String>> = places
                    .into_iter()
                    .map(|(path, label)| Choice::new(path, label))
                    .collect();

                Ok(choices)
            }
        },
    );
    // The chooser and a drop hand over several files one by one, so none is
    // dropped while another is sent.
    let (_, upload) = use_action_many(Some("files"), move |file: web_sys::File| {
        let name = file.name();
        let at = within(&path.get_untracked(), &name);
        let device = device.get_untracked();

        link.say(format!("Sending {name}…"));

        async move {
            let sent = match link::read(&file).await {
                Some(file) => device
                    .write(&at, &file)
                    .await
                    .map(|()| format!("Saved {at}")),
                None => Err(format!("read failed: {name}")),
            };

            changed.notify();

            sent
        }
    });

    ui.drops.take(move |file| upload.run(file));

    move || {
        let at = path.get();
        let start = to_pick.try_update_value(Option::take).flatten();

        view! { <Folder at start to_pick changed editing saved places upload /> }
    }
}

async fn download(
    device: Rc<dyn Device>,
    link: Link,
    saved: RwSignal<HashSet<String>>,
    paths: Vec<(String, bool)>,
) -> Result<String, String> {
    for (at, folder) in &paths {
        let name = saved_as(at, *folder);

        // The view may have closed by now. The file is saved all the same.
        saved.try_update(|saved| {
            saved.insert(at.clone());
        });

        link.say(format!("Fetching {name}…"));

        let file = device.read(at).await?;

        link::save(&name, &file);
    }

    Ok(match paths.as_slice() {
        [(at, folder)] => format!("Saved {}", saved_as(at, *folder)),
        _ => format!("Saved {}", counted(paths.len())),
    })
}
