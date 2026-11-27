use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use protocol::{At, Condition, Described, Device, DeviceId, DeviceState, Means, Route, Run, Way};
use tokio::time::sleep;

use crate::app::App;
use crate::db::Known;
use crate::known::called;

pub(crate) fn start(app: &Arc<App>) {
    app.emulators.autostart();
    tokio::spawn(devices(app.clone()));
    tokio::spawn({
        let app = app.clone();

        async move { app.apps.describe_all().await }
    });

    // Asked now so the first client does not wait for a JVM to start.
    tokio::spawn({
        let app = app.clone();

        async move {
            app.emulators.options().await;
        }
    });
}

async fn devices(app: Arc<App>) {
    loop {
        look(&app).await;
        sleep(Duration::from_secs(2)).await;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Sighting {
    pub(crate) id: DeviceId,
    pub(crate) route: Route,
    pub(crate) connected: Option<DeviceState>,
    /// Empty where the device gave none.
    pub(crate) name: String,
    pub(crate) described: Option<Described>,
}

impl Sighting {
    pub(crate) fn route(id: DeviceId, route: Route) -> Self {
        Self {
            id,
            route,
            connected: None,
            name: String::new(),
            described: None,
        }
    }
}

pub(crate) async fn look(app: &Arc<App>) {
    // Two looks at once could send their changes out of order.
    let _looking = app.looking.lock().await;
    let primary = app.trust.primary();

    app.adb.set_emulators(app.emulators.running());

    let mut seen = app.adb.sightings(&primary).await;

    app.iphones.look();
    seen.extend(app.iphones.sightings(&primary));
    seen.extend(app.browsers());

    let ready = |address: &str| {
        seen.iter()
            .any(|seen| seen.route.address == address && seen.connected == Some(DeviceState::Ready))
    };
    let mut runs = app
        .emulators
        .list(ready, |address| app.adb.connected(address));

    for (_, run) in &mut runs {
        run.remote = app.remotes.feed(&run.avd);
    }

    let place = At::Instance(primary.clone());
    let emulators: Vec<(DeviceId, Run)> = runs
        .into_iter()
        .map(|(port, run)| {
            let id = DeviceId::Emulator {
                port,
                instance: primary.clone(),
            };

            seen.push(Sighting::route(
                id.clone(),
                Route {
                    way: Way {
                        means: Means::Emulator,
                        at: place.clone(),
                    },
                    address: format!("emulator-{port}"),
                    pairing: false,
                },
            ));

            (id, run)
        })
        .collect();
    let mut devices = merge(seen, emulators, &app.known.all());
    let shown = app.devices();

    for device in &mut devices {
        let was = shown.iter().find(|was| was.id == device.id);

        if let Some(way) = just_connected(was, device) {
            device.last = Some(app.known.connected(&device.id, &device.name, &way).await);
        }
    }

    app.show(devices);
}

fn just_connected(was: Option<&Device>, now: &Device) -> Option<Way> {
    let way = now.connected().filter(|_| now.ready())?;
    let already = was.is_some_and(|was| was.ready() && was.connected() == Some(way));

    (!already).then(|| way.clone())
}

/// The order Automatic tries routes in: a cable on an instance, Wi-Fi from
/// an instance, an emulator where it runs, then a browser's USB.
fn rank(way: &Way) -> u8 {
    match (way.means, &way.at) {
        (Means::Usb, At::Instance(_)) => 0,
        (Means::Wifi, At::Instance(_)) => 1,
        (Means::Emulator, _) => 2,
        (_, At::Browser(_)) => 3,
    }
}

/// The rank of a connection's state, the best first.
fn worse(state: DeviceState) -> u8 {
    match state {
        DeviceState::Ready => 0,
        DeviceState::Unauthorized => 1,
        DeviceState::Connecting => 2,
        DeviceState::Offline => 3,
    }
}

/// A route seen twice is listed once, as it was seen first. A known device that
/// is seen no way is unavailable, except an emulator, which is listed while it
/// is one of tetherd's or runs.
pub(crate) fn merge(
    seen: Vec<Sighting>,
    emulators: Vec<(DeviceId, Run)>,
    known: &BTreeMap<DeviceId, Known>,
) -> Vec<Device> {
    let mut devices: BTreeMap<DeviceId, (Device, Option<Sighting>)> = BTreeMap::new();
    let blank = |id: &DeviceId| Device {
        id: id.clone(),
        name: String::new(),
        condition: Condition::Available,
        routes: Vec::new(),
        last: None,
        emulator: None,
    };

    for sighting in seen {
        let (device, best) = devices
            .entry(sighting.id.clone())
            .or_insert_with(|| (blank(&sighting.id), None));

        if device.name.is_empty() {
            device.name.clone_from(&sighting.name);
        }

        if device
            .routes
            .iter()
            .all(|route| route.way != sighting.route.way)
        {
            device.routes.push(sighting.route.clone());
        }

        let Some(state) = sighting.connected else {
            continue;
        };

        let better = best
            .as_ref()
            .and_then(|best| best.connected)
            .is_none_or(|kept| worse(state) < worse(kept));

        if better {
            *best = Some(sighting);
        }
    }

    for (id, run) in emulators {
        let (device, _) = devices
            .entry(id.clone())
            .or_insert_with(|| (blank(&id), None));

        device.name.clone_from(&run.avd);
        device.emulator = Some(run);
    }

    for (id, kept) in known {
        if kept.last.is_some() && !id.is_emulator() && !devices.contains_key(id) {
            devices.insert(id.clone(), (blank(id), None));
        }
    }

    devices
        .into_values()
        .map(|(mut device, best)| {
            let kept = known.get(&device.id);

            device.routes.sort_by_key(|route| rank(&route.way));
            device.last = kept.and_then(|kept| kept.last.clone());
            device.name = match kept {
                Some(kept) if kept.given.is_some() || device.name.is_empty() => {
                    called(kept).to_owned()
                }
                _ if device.name.is_empty() => device.id.serial(),
                _ => device.name,
            };

            device.condition = match best {
                Some(best) => Condition::Connected {
                    way: best.route.way,
                    state: best.connected.unwrap_or(DeviceState::Connecting),
                    described: best.described,
                },
                None if device.routes.is_empty() => Condition::Unavailable,
                None => Condition::Available,
            };

            device
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use protocol::{ClientId, InstanceId, Last, Status};

    use super::*;

    fn here() -> InstanceId {
        InstanceId("SHA256:here".to_owned())
    }

    fn way(means: Means) -> Way {
        Way {
            means,
            at: At::Instance(here()),
        }
    }

    fn browser() -> Way {
        Way {
            means: Means::Usb,
            at: At::Browser(ClientId("SHA256:firefox".to_owned())),
        }
    }

    fn seen(serial: &str, way: Way, address: &str, connected: Option<DeviceState>) -> Sighting {
        Sighting {
            connected,
            name: if connected.is_some() {
                "Pixel 9".to_owned()
            } else {
                String::new()
            },
            ..Sighting::route(
                DeviceId::Android(serial.to_owned()),
                Route {
                    way,
                    address: address.to_owned(),
                    pairing: false,
                },
            )
        }
    }

    fn known(given: Option<&str>, last: Option<Way>) -> Known {
        Known {
            name: "Pixel".to_owned(),
            given: given.map(str::to_owned),
            last: last.map(|way| Last { way, at: 1 }),
        }
    }

    #[test]
    fn lists_a_phone_seen_several_ways_once_with_its_routes_in_order() {
        let ready = Some(DeviceState::Ready);
        let devices = merge(
            vec![
                seen("AAA", browser(), "AAA", None),
                seen("AAA", way(Means::Wifi), "10.0.0.2:5555", ready),
                seen("AAA", way(Means::Usb), "AAA", None),
                // The same route again, as mDNS announces it, is left out.
                seen("AAA", way(Means::Wifi), "10.0.0.2:5555", None),
            ],
            Vec::new(),
            &BTreeMap::new(),
        );

        assert_eq!(devices.len(), 1);

        let device = &devices[0];
        let ways: Vec<&Way> = device.routes.iter().map(|route| &route.way).collect();

        assert_eq!(ways, [&way(Means::Usb), &way(Means::Wifi), &browser()]);
        assert_eq!(device.connected(), Some(&way(Means::Wifi)));
        assert!(device.ready());
        assert_eq!(device.name, "Pixel 9");
        assert_eq!(device.last, None);
    }

    #[test]
    fn tells_where_each_device_stands() {
        let id = |serial: &str| DeviceId::Android(serial.to_owned());
        let known = BTreeMap::from([
            (
                id("KNOWN"),
                known(Some("Desk phone"), Some(way(Means::Usb))),
            ),
            (id("AWAY"), known(None, Some(way(Means::Wifi)))),
            // Renamed and never connected, so it is new while it is seen
            // and not listed while it is not.
            (id("UNSEEN"), known(Some("Spare"), None)),
        ]);
        let devices = merge(
            vec![
                seen("KNOWN", way(Means::Wifi), "10.0.0.3:5555", None),
                seen("NEW", way(Means::Usb), "NEW", None),
                seen(
                    "ASKING",
                    way(Means::Usb),
                    "ASKING",
                    Some(DeviceState::Unauthorized),
                ),
                seen("ASKING", browser(), "ASKING", Some(DeviceState::Ready)),
            ],
            Vec::new(),
            &known,
        );
        let stood: Vec<(String, &str, Condition, bool)> = devices
            .iter()
            .map(|device| {
                let serial = device.id.serial();
                let condition = match &device.condition {
                    Condition::Connected { .. } => Condition::Connected {
                        way: device.connected().unwrap().clone(),
                        state: DeviceState::Ready,
                        described: None,
                    },
                    other => other.clone(),
                };

                (
                    serial,
                    device.name.as_str(),
                    condition,
                    device.last.is_some(),
                )
            })
            .collect();

        assert_eq!(
            stood,
            [
                (
                    "ASKING".to_owned(),
                    "Pixel 9",
                    Condition::Connected {
                        way: browser(),
                        state: DeviceState::Ready,
                        described: None,
                    },
                    false
                ),
                ("AWAY".to_owned(), "Pixel", Condition::Unavailable, true),
                ("KNOWN".to_owned(), "Desk phone", Condition::Available, true),
                ("NEW".to_owned(), "NEW", Condition::Available, false),
            ]
        );
    }

    #[test]
    fn lists_an_emulator_by_its_virtual_device_whether_it_runs_or_not() {
        let id = DeviceId::Emulator {
            port: 5554,
            instance: here(),
        };
        let run = Run {
            avd: "pixel".to_owned(),
            status: Status::Shutdown,
            upright: true,
            camera: None,
            fed_by: None,
            remote: None,
        };
        let route = Sighting::route(
            id.clone(),
            Route {
                way: way(Means::Emulator),
                address: "emulator-5554".to_owned(),
                pairing: false,
            },
        );
        // A connected emulator once known stays unlisted when it is gone.
        let known = BTreeMap::from([(
            DeviceId::Emulator {
                port: 5556,
                instance: here(),
            },
            known(None, Some(way(Means::Emulator))),
        )]);
        let devices = merge(vec![route], vec![(id.clone(), run.clone())], &known);

        assert_eq!(
            devices,
            [Device {
                id,
                name: "pixel".to_owned(),
                condition: Condition::Available,
                routes: vec![Route {
                    way: way(Means::Emulator),
                    address: "emulator-5554".to_owned(),
                    pairing: false,
                }],
                last: None,
                emulator: Some(run),
            }]
        );
    }

    #[test]
    fn keeps_the_way_of_a_device_once_it_is_ready() {
        let device = |condition| Device {
            id: DeviceId::Android("AAA".to_owned()),
            name: "Pixel".to_owned(),
            condition,
            routes: Vec::new(),
            last: None,
            emulator: None,
        };
        let connected = |means, state| Condition::Connected {
            way: way(means),
            state,
            described: None,
        };
        let ready = device(connected(Means::Usb, DeviceState::Ready));

        assert_eq!(just_connected(None, &ready), Some(way(Means::Usb)));
        assert_eq!(
            just_connected(
                Some(&device(connected(Means::Usb, DeviceState::Connecting))),
                &ready
            ),
            Some(way(Means::Usb))
        );

        assert_eq!(just_connected(Some(&ready), &ready), None);
        assert_eq!(
            just_connected(
                Some(&device(connected(Means::Wifi, DeviceState::Ready))),
                &ready
            ),
            Some(way(Means::Usb))
        );

        assert_eq!(just_connected(None, &device(Condition::Available)), None);
    }
}
