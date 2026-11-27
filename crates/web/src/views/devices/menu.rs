use leptos::prelude::*;
use protocol::{Condition, Device, DeviceId, Means, Status};

use super::{delete, listed};
use crate::app::{Global, Page, icons};
use crate::link::means;
use crate::ui::menu::{Item, Row};
use crate::ui::renamable::Renaming;

pub(super) fn menu_items(device: &Device, renaming: Renaming<DeviceId>) -> Vec<Item> {
    let Global { link, router, ui } = expect_context::<Global>();
    let id = device.id.clone();
    let mut items = Vec::new();

    if let Some(run) = &device.emulator {
        let place = match &device.id {
            DeviceId::Emulator { instance, .. } => Some(untrack(|| link.instance_name(instance))),
            _ => None,
        };
        let emulator = device.id.clone();
        // A stopped emulator's row names what it does, since "Stopped" says
        // nothing a click could change.
        let (what, icon, does, ticked) = match run.status {
            Status::Booted => ("Running", icons::PLAY, Some("Stop"), true),
            Status::Shutdown => ("Start", icons::PLAY, Some("Start"), false),
            Status::Booting => ("Booting…", icons::PLAY, None, false),
            Status::Stopping => ("Stopping…", icons::SQUARE, None, false),
        };

        items.push(Item::Row(Row {
            what: what.to_owned(),
            icon: Some(icon),
            does_icon: Some(if ticked { icons::SQUARE } else { icons::PLAY }),
            place,
            ticked,
            does: does.map(str::to_owned),
            undoes: ticked,
            on: Callback::new(move |()| {
                if ticked {
                    link.shut_down(emulator.clone());
                } else {
                    link.boot(emulator.clone());
                }
            }),
        }));
    } else if device.condition != Condition::Unavailable {
        let now = device.connected();

        items.push(Item::Label("Connect through"));

        for route in &device.routes {
            let kind = means(route.way.means);
            let ticked = now == Some(&route.way);
            let does = if ticked {
                "Disconnect".to_owned()
            } else if route.pairing {
                format!("Pair through {kind}")
            } else if now.is_some() {
                format!("Switch to {kind}")
            } else {
                format!("Connect through {kind}")
            };
            let place = Some(untrack(|| link.place(&route.way.at)));
            let id = id.clone();
            let way = route.way.clone();

            let icon = match route.way.means {
                Means::Wifi => icons::WIFI,
                Means::Usb | Means::Emulator => icons::USB,
            };

            items.push(Item::Row(Row {
                what: kind.to_owned(),
                icon: Some(icon),
                does_icon: Some(icon),
                place,
                ticked,
                does: Some(does),
                undoes: ticked,
                on: Callback::new(move |()| {
                    if ticked {
                        link.disconnect(id.clone());
                    } else {
                        link.connect_to(ui, id.clone(), Some(way.clone()));
                    }
                }),
            }));
        }
    }

    if listed::new(device) {
        return items;
    }

    if !items.is_empty() {
        items.push(Item::Rule);
    }

    if let Some(run) = &device.emulator {
        let id = id.clone();
        let avd = run.avd.clone();

        items.push(Item::Keyed {
            text: "Configure",
            keys: None,
            red: false,
            on: Callback::new(move |()| {
                router.open(
                    id.clone(),
                    Page::Devices {
                        emulator: Some(avd.clone()),
                    },
                );
            }),
        });
    }

    let device = device.clone();

    items.push(Item::Keyed {
        text: "Rename",
        keys: Some("F2"),
        red: false,
        on: Callback::new(move |()| renaming.start(id.clone())),
    });
    items.push(Item::Keyed {
        text: "Delete",
        keys: Some("Del"),
        red: true,
        on: Callback::new(move |()| delete(ui, link, &device)),
    });

    items
}
