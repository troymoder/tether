use leptos::prelude::*;
use stylance::classes;
use web_sys::Url;

use crate::app::{Global, Icon, Picked, icons};
use crate::link;
use crate::ui;
use crate::ui::action::use_action_many;
use crate::ui::button::Button;
use crate::ui::file_pick::{FilePick, FilePicker};
use crate::ui::load::use_load_local;
use crate::ui::loadable::Status;
use crate::ui::pane::Pane;

mod device;
mod gallery;
mod stream;
mod thumbnails;
mod tile;
mod tiles;
mod viewer;

pub(crate) use device::{Item, Media};
use gallery::Gallery;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "media.module.scss"
);

struct Address(String);

impl Address {
    fn of(file: &[u8]) -> Option<Self> {
        link::address(file).map(Address)
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for Address {
    fn drop(&mut self) {
        let _ = Url::revoke_object_url(&self.0);
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Media() -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let Picked { device, listed, .. } = expect_context::<Picked>();
    let changed = Trigger::new();
    // The file chooser and a drop hand over several files one by one.
    // `use_action_many` runs them side by side, so no file is dropped while
    // another is being added.
    let (_, add) = use_action_many(Some("media"), move |file: web_sys::File| {
        let name = file.name();
        let device = device.get_untracked();

        link.say(format!("Adding {name}…"));

        async move {
            let sent = match link::read(&file).await {
                Some(file) => device.add(&name, &file).await,
                None => Err(format!("read failed: {name}")),
            };

            changed.notify();

            sent
        }
    });

    ui.drops.take(move |file| add.run(file));

    // The listing also changes for other reasons, such as an emulator being
    // rotated. Only a new connection needs a new helper.
    let reached = Memo::new(move |_| listed.with(|listed| listed.connected().cloned()));
    let helper = use_load_local(
        move || reached.get(),
        move |_| {
            let device = device.get_untracked();

            async move { device.gallery().await }
        },
    );

    // The gallery is built again for each helper, so nothing read through an
    // old helper outlives it.
    view! {
        <Status of=helper line=move |said: String| waiting(add, said) let:helper>
            <Gallery helper changed add />
        </Status>
    }
}

fn waiting(add: Callback<web_sys::File>, said: String) -> AnyView {
    view! {
        <Pane
            title="Media"
            under=move || {
                view! {
                    <div class=style::row>
                        <Upload add />
                    </div>
                }
            }
        >
            <div class=style::under>
                <div class=classes!(ui::style::pane_body, style::body)>
                    <p class=ui::style::hint>{said}</p>
                </div>
            </div>
        </Pane>
    }
    .into_any()
}

#[allow(unreachable_pub)]
#[component]
fn Upload(add: Callback<web_sys::File>) -> impl IntoView {
    let chooser = FilePicker::new();

    view! {
        <FilePick
            picker=chooser
            accept="image/*,video/*"
            multiple=true
            label="Photos and videos for the gallery"
            on_file=add
        />
        <Button
            class=style::upload
            attr:title="Upload photos or videos"
            attr:aria-label="Upload photos or videos"
            on:click=move |_| chooser.open()
        >
            <Icon paths=icons::UPLOAD />
            <span>"Upload"</span>
        </Button>
    }
}
