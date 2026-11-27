//! Cards state the device's condition only. The tooltip says what a click
//! does.

use protocol::{At, Condition, Device, DeviceState, Platform, Status};

use crate::link::means;
use crate::ui::search;
use crate::ui::tag::Dot;
use crate::views::access::ago;
use crate::views::settings::image_name;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Online,
    Available,
    Elsewhere,
    Unavailable,
}

impl Kind {
    pub(super) const ALL: [Kind; 4] = [
        Kind::Online,
        Kind::Available,
        Kind::Elsewhere,
        Kind::Unavailable,
    ];

    pub(super) fn name(self) -> &'static str {
        match self {
            Kind::Online => "Online",
            Kind::Available => "Available",
            Kind::Elsewhere => "In use elsewhere",
            Kind::Unavailable => "Unavailable",
        }
    }

    pub(super) fn covers(self) -> Option<&'static str> {
        match self {
            Kind::Online => Some("Booted, Connected, Booting, Stopping"),
            Kind::Available => Some("Available, Pairing, Shutdown"),
            Kind::Elsewhere | Kind::Unavailable => None,
        }
    }
}

pub(super) fn new(device: &Device) -> bool {
    device.last.is_none() && device.emulator.is_none()
}

pub(super) fn pairs(device: &Device) -> bool {
    device.routes.first().is_some_and(|route| route.pairing)
}

pub(super) fn tag(device: &Device) -> (&'static str, Dot, Kind) {
    if let Some(run) = &device.emulator {
        return match run.status {
            Status::Booted => ("Booted", Dot::On, Kind::Online),
            Status::Booting => ("Booting", Dot::Starting, Kind::Online),
            Status::Stopping => ("Stopping", Dot::Starting, Kind::Online),
            Status::Shutdown => ("Shutdown", Dot::Ring, Kind::Available),
        };
    }

    match &device.condition {
        Condition::Connected { state, .. } => match state {
            DeviceState::Ready => ("Connected", Dot::On, Kind::Online),
            DeviceState::Unauthorized => ("Unauthorized", Dot::Starting, Kind::Online),
            DeviceState::Connecting => ("Connecting", Dot::Starting, Kind::Online),
            DeviceState::Offline => ("Offline", Dot::Ring, Kind::Online),
        },
        Condition::Available if pairs(device) => ("Pairing", Dot::FoundRing, Kind::Available),
        Condition::Available => ("Available", Dot::Found, Kind::Available),
        Condition::InUse => ("In use elsewhere", Dot::Warn, Kind::Elsewhere),
        Condition::Unavailable => ("Unavailable", Dot::Ring, Kind::Unavailable),
    }
}

/// `at` is in milliseconds since 1970.
fn day(at: u64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(at as f64));
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &"day".into(), &"numeric".into());
    let _ = js_sys::Reflect::set(&options, &"month".into(), &"short".into());

    date.to_locale_date_string("en-GB", &options).into()
}

fn places(device: &Device) -> usize {
    let mut seen: Vec<&At> = Vec::new();

    for route in &device.routes {
        if !seen.contains(&&route.way.at) {
            seen.push(&route.way.at);
        }
    }

    seen.len()
}

pub(super) fn about(device: &Device, specs: Option<&protocol::EmulatorSettings>) -> Option<String> {
    if device.emulator.is_some() {
        return specs.map(|specs| {
            format!(
                "{} · {} · {} GB",
                specs.profile,
                image_name(&specs.image),
                specs.memory / 1024
            )
        });
    }

    match &device.condition {
        Condition::Connected {
            way,
            state,
            described,
        } => {
            let ios = device.id.platform() == Platform::Ios;

            Some(match (state, described) {
                (DeviceState::Ready, Some(described)) if ios => {
                    format!("iOS {} · {}", described.release, means(way.means))
                }
                (DeviceState::Ready, Some(described)) => format!(
                    "Android {} · {} · {}",
                    described.release,
                    described.abis.join(", "),
                    means(way.means)
                ),
                (DeviceState::Ready, None) => means(way.means).to_owned(),
                (DeviceState::Unauthorized, _) if ios => "Tap Trust on the iPhone".to_owned(),
                (DeviceState::Unauthorized, _) => {
                    "USB debugging not authorized: accept the prompt on the device".to_owned()
                }
                (DeviceState::Connecting, _) => "Connecting…".to_owned(),
                (DeviceState::Offline, _) => "adb: offline".to_owned(),
            })
        }
        _ if new(device) && places(device) > 1 => {
            Some(format!("Seen from {} places", places(device)))
        }
        _ if new(device) => None,
        Condition::Available => device
            .last
            .as_ref()
            .map(|last| format!("Connected before, {}", ago(last.at))),
        Condition::InUse => Some("In use elsewhere".to_owned()),
        Condition::Unavailable => device
            .last
            .as_ref()
            .map(|last| format!("Last connected {}", day(last.at))),
    }
}

pub(super) fn tooltip(device: &Device) -> Option<&'static str> {
    if let Some(run) = &device.emulator {
        return (run.status == Status::Shutdown).then_some("Double-click to boot");
    }

    match device.condition {
        _ if new(device) && pairs(device) => Some("Click to pair"),
        _ if new(device) => Some("Click to connect"),
        Condition::Available | Condition::InUse => Some("Double-click to connect"),
        Condition::Connected { .. } | Condition::Unavailable => None,
    }
}

pub(super) fn rank(device: &Device) -> u8 {
    match (&device.emulator, &device.condition) {
        (Some(_), _) => 0,
        (None, Condition::Connected { .. }) => 1,
        (None, Condition::Available) => 2,
        (None, Condition::InUse) => 3,
        (None, Condition::Unavailable) => 4,
    }
}

pub(super) fn matches(device: &Device, sought: &str, hidden: &[Kind]) -> bool {
    !hidden.contains(&tag(device).2)
        && search::matches(sought, &[&device.name, &device.id.serial()])
}

#[cfg(test)]
mod tests {
    use protocol::{DeviceId, Last, Means, Way};

    use super::*;

    fn away() -> Device {
        Device {
            id: DeviceId::Android("R58M123".to_owned()),
            name: "Pixel 7".to_owned(),
            condition: Condition::Unavailable,
            routes: Vec::new(),
            last: Some(Last {
                way: Way {
                    means: Means::Usb,
                    at: At::Instance(protocol::InstanceId("SHA256:primary".to_owned())),
                },
                at: 0,
            }),
            emulator: None,
        }
    }

    #[test]
    fn matches_the_name_or_serial_unless_its_kind_is_hidden() {
        let away = away();

        assert!(matches(&away, "", &[]));
        assert!(matches(&away, "pixel", &[]));
        assert!(matches(&away, "r58m", &[]));
        assert!(!matches(&away, "galaxy", &[]));
        assert!(matches(&away, "", &[Kind::Online, Kind::Elsewhere]));
        assert!(!matches(&away, "", &[Kind::Unavailable]));
    }

    #[test]
    fn a_card_has_a_line_of_detail_only_when_there_is_something_to_say() {
        let never = Device {
            condition: Condition::Available,
            last: None,
            ..away()
        };

        assert_eq!(about(&never, None), None);
        assert_eq!(
            about(
                &Device {
                    condition: Condition::InUse,
                    ..away()
                },
                None
            ),
            Some("In use elsewhere".to_owned())
        );
    }

    #[test]
    fn other_devices_go_connected_known_in_use_unavailable() {
        let mut devices = [
            Condition::Unavailable,
            Condition::InUse,
            Condition::Available,
        ]
        .map(|condition| Device {
            condition,
            ..away()
        });

        devices.sort_by_key(rank);

        assert!(!new(&devices[0]));
        assert_eq!(
            devices.map(|device| device.condition),
            [
                Condition::Available,
                Condition::InUse,
                Condition::Unavailable
            ]
        );
    }
}
