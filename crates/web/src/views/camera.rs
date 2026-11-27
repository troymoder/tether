//! An emulator of this tetherd has its virtual camera fed from this browser
//! or from tetherd's machine (fed.rs). Any other device's own cameras are
//! mirrored (phone.rs).

use leptos::prelude::*;
use protocol::Topic;

use crate::app::{Global, Picked};
use crate::device::unsupported;
use crate::ui;
use crate::ui::pane::Pane;

mod browser;
mod device;
mod fed;
mod geometry;
mod holder;
mod mirror;
mod mode;
mod phone;
mod pick;
mod source;
mod source_box;
mod source_settings;
mod stage;
mod turning;
mod zoom;

pub(crate) use device::{Camera, Control, Emulator, Kind, Mirrored};

stylance::import_style!(
    #[allow(dead_code, unreachable_pub)]
    style,
    "camera.module.scss"
);

/// `Kind` without what comes with it. The view is built again only when
/// this changes, so it stays up as an emulator's feed or settings change.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Which {
    None,
    Fed,
    Mirrored,
}

impl Which {
    fn of(kind: &Kind) -> Self {
        match kind {
            Kind::None => Which::None,
            Kind::Fed(_) => Which::Fed,
            Kind::Mirrored => Which::Mirrored,
        }
    }
}

fn said(what: String) -> impl IntoView {
    view! {
        <Pane title="Camera" class=style::view>
            <div class=ui::style::pane_body>
                <div class=style::stage>
                    <span class=style::saying role="status">
                        {what}
                    </span>
                </div>
            </div>
        </Pane>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Camera() -> impl IntoView {
    let link = expect_context::<Global>().link;
    let Picked { device, listed, .. } = expect_context::<Picked>();
    // An emulator's settings say which cameras it has.
    let avd = Memo::new(move |_| {
        listed.with(|listed| listed.emulator.as_ref().map(|run| run.avd.clone()))
    });

    link.follow(move || avd.get().map(Topic::Emulator));

    let kind = Memo::new(move |_| Camera::kind(&*device.get()));
    let which = Memo::new(move |_| kind.with(Which::of));

    move || match which.get() {
        Which::None => said(unsupported("camera")).into_any(),
        Which::Mirrored => view! { <phone::Phone /> }.into_any(),
        Which::Fed => match kind.get_untracked() {
            Kind::Fed(first) => {
                // It keeps what it last was for the moment in which the device
                // stops being one that is fed, before this is taken down.
                let emulator = Memo::new(move |last: Option<&Emulator>| match kind.get() {
                    Kind::Fed(now) => now,
                    Kind::None | Kind::Mirrored => last.cloned().unwrap_or_else(|| first.clone()),
                });

                view! { <fed::Fed emulator /> }.into_any()
            }
            Kind::None | Kind::Mirrored => ().into_any(),
        },
    }
}
