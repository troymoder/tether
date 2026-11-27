//! Only these functions connect, and only when the user asks. Nothing
//! reconnects to a device on its own.

use leptos::prelude::*;
use leptos::task::spawn_local;
use protocol::{Condition, DeviceId, Request, Route, Status, Way};

use super::Link;
use super::pending::Action;
use crate::app::{Adding, Ui};

impl Link {
    /// A `way` of None takes the first of the device's routes, as Automatic
    /// does.
    // The calling handler passes `ui`, because the popup and the question
    // belong to the page's UI and the link has no handle to it.
    pub(crate) fn connect_to(self, ui: Ui, id: DeviceId, way: Option<Way>) {
        let Some(device) = untrack(|| self.device(&id)) else {
            return;
        };

        if let Some(run) = &device.emulator {
            if run.status == Status::Shutdown {
                self.boot(id);
            }

            return;
        }

        let route: Option<Route> = match &way {
            Some(way) => device.routes.iter().find(|route| route.way == *way),
            None => device.routes.first(),
        }
        .cloned();
        let Some(route) = route else {
            return;
        };

        if route.pairing {
            ui.adding.set(Some(Adding::Pairing(id, route)));
            return;
        }

        let in_use = device.condition == Condition::InUse;
        // Picking another route of a connected device switches to it, which
        // drops the one in use.
        let switching = device.connected().is_some();

        spawn_local(async move {
            if in_use
                && !ui
                    .agreed(
                        format!("Connect to {}?", device.name),
                        "It's in use elsewhere and loses it.",
                        "Connect",
                    )
                    .await
            {
                return;
            }

            let take = in_use || switching;

            if untrack(|| self.here(&route.way)) {
                self.use_usb(&id.serial(), take);
            } else {
                let request = Request::Connect {
                    device: id.clone(),
                    way,
                    code: None,
                    take,
                };

                self.act(id, Action::Connect(route.way), request);
            }
        });
    }

    /// A phone on this browser's WebUSB is released here, and the primary
    /// tetherd learns of it from the next report.
    pub(crate) fn disconnect(self, id: DeviceId) {
        if self.held(&id.serial()).is_some() {
            self.let_go(&id.serial());
        } else {
            let request = Request::Disconnect { device: id.clone() };

            self.act(id, Action::Disconnect, request);
        }
    }
}
