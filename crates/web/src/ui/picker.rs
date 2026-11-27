//! The picker lists only what is there to use now. Anything else is picked
//! from its card, and the picked device stays listed so that the picker can
//! name it.

use leptos::prelude::*;
use protocol::{Condition, Device, DeviceId, DeviceState, Status};

use crate::app::Global;
use crate::link::{ServerStoreFields, means};
use crate::ui::dropdown::{Choice, Dropdown};
use crate::ui::tag::Dot;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "picker.module.scss"
);

// The words are the cards' tags in views/devices/listed.rs.
const BOOTED: (Dot, &str) = (Dot::On, "Booted");
const CONNECTED: (Dot, &str) = (Dot::On, "Connected");
const BOOTING: (Dot, &str) = (Dot::Starting, "Booting");
const CONNECTING: (Dot, &str) = (Dot::Starting, "Connecting");
const STOPPING: (Dot, &str) = (Dot::Starting, "Stopping");
const OFF: (Dot, &str) = (Dot::Ring, "Shutdown");
const OFFLINE: (Dot, &str) = (Dot::Ring, "Offline");
const AWAY: (Dot, &str) = (Dot::Found, "Available");
const IN_USE: (Dot, &str) = (Dot::Warn, "In use elsewhere");
const UNAVAILABLE: (Dot, &str) = (Dot::Ring, "Unavailable");

/// The one in use comes first, as "USB · Wi-Fi".
pub(crate) fn routes(device: &Device) -> String {
    let now = device.connected();
    let mut ways: Vec<&'static str> = now.map(|way| means(way.means)).into_iter().collect();

    for route in &device.routes {
        let named = means(route.way.means);

        if Some(&route.way) != now && !ways.contains(&named) {
            ways.push(named);
        }
    }

    ways.join(" · ")
}

fn devices(listed: &[Device], picked: Option<&DeviceId>) -> Vec<Choice<DeviceId>> {
    listed
        .iter()
        .filter_map(|device| {
            let (dot, usable) = match (&device.emulator, &device.condition) {
                (Some(run), _) => match run.status {
                    Status::Booted => (BOOTED, true),
                    Status::Booting => (BOOTING, true),
                    Status::Stopping => (STOPPING, true),
                    Status::Shutdown => (OFF, false),
                },
                (None, Condition::Connected { state, .. }) => match state {
                    DeviceState::Ready => (CONNECTED, true),
                    DeviceState::Unauthorized | DeviceState::Connecting => (CONNECTING, true),
                    DeviceState::Offline => (OFFLINE, true),
                },
                (None, Condition::Available) => (AWAY, false),
                (None, Condition::InUse) => (IN_USE, false),
                (None, Condition::Unavailable) => (UNAVAILABLE, false),
            };

            if !usable && Some(&device.id) != picked {
                return None;
            }

            // Only a phone reached more than one way has its routes listed.
            let ways = routes(device);

            Some(Choice {
                dot: Some(dot),
                detail: ways.contains(" · ").then_some(ways),
                ..Choice::new(device.id.clone(), device.name.clone()).noted(dot.1)
            })
        })
        .collect()
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Picker() -> impl IntoView {
    let Global { link, router, .. } = expect_context::<Global>();
    let choices = Memo::new(move |_| {
        let picked = router.device();

        link.server
            .devices()
            .with(|listed| devices(listed, picked.as_ref()))
    });

    view! {
        <Dropdown
            class=style::picker
            label="Device"
            // Blank until the primary has sent its state, so it never claims
            // there is no device too early.
            placeholder=Signal::derive(move || link.loaded.get().then_some("No device"))
            choices
            chosen=Signal::derive(move || router.device())
            on_choose=move |device: DeviceId| router.pick(Some(device))
        />
    }
}
