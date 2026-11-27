//! The copy is a store, so a reader of one topic, or of one device, app or
//! emulator's settings, does not rerun when another changes.

use std::collections::HashMap;

use leptos::prelude::*;
use protocol::{
    Access, App, At, ClientId, Condition, Device, DeviceId, EmulatorSettings, InstanceId, Means,
    Place, Run, Way,
};
use reactive_stores::Store;

use super::Link;

#[derive(Clone, Debug, Default, Store)]
pub(crate) struct Server {
    #[store(key: DeviceId = |device| device.id.clone())]
    pub devices: Vec<Device>,
    #[store(key: String = |app| app.id.clone())]
    pub apps: Vec<App>,
    pub places: Vec<Place>,
    /// None until tetherd has sent it after the page connects.
    pub access: Option<Access>,
    #[store(key: String = |(avd, _)| avd.clone())]
    pub settings: HashMap<String, EmulatorSettings>,
}

impl Link {
    pub(crate) fn device(self, id: &DeviceId) -> Option<Device> {
        self.server.devices().at_key(id.clone()).try_get()
    }

    pub(crate) fn run(self, id: &DeviceId) -> Option<Run> {
        self.device(id)?.emulator
    }

    pub(crate) fn emulator(self, avd: &str) -> Option<Device> {
        self.server.devices().with(|devices| {
            devices
                .iter()
                .find(|device| device.emulator.as_ref().is_some_and(|run| run.avd == avd))
                .cloned()
        })
    }

    pub(crate) fn settings(self, avd: &str) -> Option<EmulatorSettings> {
        self.server.settings().at_key(avd.to_owned()).try_get()
    }

    pub(crate) fn name(self, id: &DeviceId) -> String {
        self.device(id)
            .map_or_else(|| id.serial(), |device| device.name)
    }

    pub(crate) fn about(self) -> Option<DeviceId> {
        let picked = self.picked.get();

        self.server
            .devices()
            .with(|devices| about(devices, picked.as_ref()))
    }

    pub(crate) fn primary(self) -> Option<InstanceId> {
        self.server
            .access()
            .with(|access| access.as_ref().map(|access| access.primary.clone()))
    }

    pub(crate) fn primary_name(self) -> String {
        match self.primary() {
            Some(primary) => self.instance_name(&primary),
            None => host(),
        }
    }

    pub(crate) fn instance_name(self, id: &InstanceId) -> String {
        self.server.access().with(|access| {
            let named = access.as_ref().and_then(|access| {
                let instance = access.instances.iter().find(|one| one.id == *id)?;

                instance.name.clone()
            });
            let primary = access.as_ref().map(|access| &access.primary) == Some(id);

            named.unwrap_or_else(|| if primary { host() } else { id.0.clone() })
        })
    }

    pub(crate) fn client_name(self, id: &ClientId) -> String {
        self.server.access().with(|access| {
            let trusted = access.iter().flat_map(|access| &access.trusted);
            let named = trusted.into_iter().find(|client| client.print == id.0);

            named.map_or_else(|| id.0.clone(), |client| client.name.clone())
        })
    }

    pub(crate) fn me(self, print: &str) -> bool {
        self.you
            .with(|you| you.as_ref().is_some_and(|you| you.0 == print))
    }

    pub(crate) fn here(self, way: &Way) -> bool {
        matches!(&way.at, At::Browser(client) if self.me(&client.0))
    }

    pub(crate) fn place(self, at: &At) -> String {
        match at {
            At::Browser(client) if self.me(&client.0) => "this browser".to_owned(),
            At::Browser(client) => self.client_name(client),
            At::Instance(instance) => self.instance_name(instance),
        }
    }

    pub(crate) fn mentioned(self, message: &str) -> String {
        protocol::mentioned(message, |id| self.name(id))
    }

    pub(super) fn put_device(self, device: Device) {
        let devices = self.server.devices();
        let known = devices.at_key(device.id.clone());

        if known.try_read_untracked().is_some() {
            known.set(device);
        } else {
            devices.write().push(device);
        }
    }

    pub(super) fn put_app(self, app: App) {
        let apps = self.server.apps();
        let known = apps.at_key(app.id.clone());

        if known.try_read_untracked().is_some() {
            known.set(app);
        } else {
            apps.write().push(app);
        }
    }

    pub(super) fn put_settings(self, avd: String, settings: Option<EmulatorSettings>) {
        let kept = self.server.settings();
        let known = kept.at_key(avd.clone());

        match settings {
            Some(settings) if known.try_read_untracked().is_some() => known.set(settings),
            Some(settings) => {
                kept.write().insert(avd, settings);
            }
            None => {
                kept.write().remove(&avd);
            }
        }
    }
}

fn host() -> String {
    window().location().hostname().unwrap_or_default()
}

fn about(devices: &[Device], pick: Option<&DeviceId>) -> Option<DeviceId> {
    // A device connected before stays pickable in any state, so it can be
    // connected, renamed or forgotten, and so does an emulator. A new one
    // cannot be picked until it has connected.
    let pickable =
        |device: &&Device| device.ready() || device.last.is_some() || device.emulator.is_some();
    let picked = devices
        .iter()
        .find(|device| Some(&device.id) == pick)
        .filter(pickable);
    let first = || {
        let ready = devices.iter().find(|device| device.ready());

        ready.or_else(|| devices.iter().find(pickable))
    };

    picked.or_else(first).map(|device| device.id.clone())
}

pub(crate) fn means(means: Means) -> &'static str {
    match means {
        Means::Usb => "USB",
        Means::Wifi => "Wi-Fi",
        Means::Emulator => "Emulator",
    }
}

/// A device connected through another browser's WebUSB is in use elsewhere,
/// because only that browser can use it. tetherd lists it as connected there
/// and never as in use.
pub(crate) fn seen_from(mut device: Device, you: Option<&ClientId>) -> Device {
    let elsewhere = matches!(
        &device.condition,
        Condition::Connected { way: Way { at: At::Browser(client), .. }, .. } if Some(client) != you
    );

    if elsewhere {
        device.condition = Condition::InUse;
    }

    device
}

#[cfg(test)]
mod tests {
    use protocol::Route;

    use super::*;

    fn phone(serial: &str, name: &str) -> Device {
        Device {
            id: DeviceId::Android(serial.to_owned()),
            name: name.to_owned(),
            condition: Condition::Available,
            routes: Vec::<Route>::new(),
            last: None,
            emulator: None,
        }
    }

    fn on(print: &str) -> Device {
        Device {
            condition: Condition::Connected {
                way: Way {
                    means: Means::Usb,
                    at: At::Browser(ClientId(print.to_owned())),
                },
                state: protocol::DeviceState::Ready,
                described: None,
            },
            ..phone("A", "Pixel")
        }
    }

    #[test]
    fn the_pick_holds_while_it_can_be_picked() {
        let known = Device {
            last: Some(protocol::Last {
                way: Way {
                    means: Means::Usb,
                    at: At::Browser(ClientId("SHA256:me".to_owned())),
                },
                at: 0,
            }),
            ..phone("B", "Galaxy")
        };
        let devices = [phone("N", "New"), known.clone(), on("SHA256:me")];
        let ready = Some(DeviceId::Android("A".to_owned()));

        // A device that was connected before stays picked while it is away.
        assert_eq!(about(&devices, Some(&known.id)), Some(known.id.clone()));
        // A new one cannot be picked, and neither can one that is not
        // listed, so the first ready device is picked.
        assert_eq!(about(&devices, Some(&devices[0].id)), ready);
        assert_eq!(
            about(&devices, Some(&DeviceId::Android("Z".to_owned()))),
            ready
        );
        assert_eq!(about(&devices, None), ready);
        assert_eq!(about(&devices[..1], None), None);
    }

    #[test]
    fn a_phone_on_another_browser_is_in_use_elsewhere() {
        let me = ClientId("SHA256:me".to_owned());

        assert_eq!(
            seen_from(on("SHA256:other"), Some(&me)).condition,
            Condition::InUse
        );
        assert!(seen_from(on("SHA256:me"), Some(&me)).connected().is_some());
    }
}
