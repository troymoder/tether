use leptos::prelude::*;
use protocol::{Condition, Device, DeviceId, Request, Status};

use crate::app::{Global, Page, Ui};
use crate::link::Link;

mod arrows;
mod card;
mod connect;
mod emulator;
mod list;
mod listed;
mod menu;
mod scan;
mod states;

pub(crate) use connect::Connect;
use emulator::Emulator;
use list::List;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "devices.module.scss"
);

/// True if a double click on its card connects or boots it.
fn starts(device: &Device) -> bool {
    match (&device.emulator, &device.condition) {
        (Some(run), _) => run.status == Status::Shutdown,
        (None, Condition::Available | Condition::InUse) => true,
        (None, Condition::Connected { .. } | Condition::Unavailable) => false,
    }
}

// `ui` and `link` are given by the handler that calls this, which has no
// owner to read the context from.
pub(crate) fn delete(ui: Ui, link: Link, device: &Device) {
    let title = format!("Delete {}?", device.name);
    let (body, request) = match &device.emulator {
        Some(run) => (
            "Removes it and everything on it.",
            Request::Delete {
                name: run.avd.clone(),
            },
        ),
        None => (
            "The phone isn't erased. tetherd drops what it keeps for it: its name and Auto-update \
             on its apps. Connecting it again brings it back as a new device.",
            Request::ForgetDevice {
                device: device.id.clone(),
            },
        ),
    };

    ui.confirm_then(title, body, "Delete", async move { link.ask(request) });
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Devices() -> impl IntoView {
    let Global { link, router, .. } = expect_context::<Global>();
    let configured = Memo::new(move |_| {
        let avd = router.on(|page| match page {
            Page::Devices { emulator } => emulator.clone(),
            _ => None,
        })?;

        match link.emulator(&avd)?.id {
            DeviceId::Emulator { instance, .. } => Some((avd, instance)),
            DeviceId::Android(_) | DeviceId::Iphone(_) => None,
        }
    });

    move || match configured.get() {
        Some((avd, instance)) => view! { <Emulator avd instance /> }.into_any(),
        None => view! { <List /> }.into_any(),
    }
}
