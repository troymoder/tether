//! The place the picked device was put at and its route are kept in
//! `Picked::routes`. They belong to the device, so a route goes on while
//! another view is open and ends when another device is picked.

use std::rc::Rc;
use std::time::Duration;

use leptos::prelude::*;
use protocol::DeviceId;

use super::route::{along, length};
use crate::device::Device;
use crate::link::Link;
use crate::util::sleep;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Speed {
    Walking,
    Running,
    Cycling,
    Town,
    Motorway,
    Train,
}

impl Speed {
    pub(super) const ALL: [Speed; 6] = [
        Speed::Walking,
        Speed::Running,
        Speed::Cycling,
        Speed::Town,
        Speed::Motorway,
        Speed::Train,
    ];

    pub(super) fn metres_a_second(self) -> f64 {
        match self {
            Speed::Walking => 1.4,
            Speed::Running => 2.8,
            Speed::Cycling => 5.6,
            Speed::Town => 13.9,
            Speed::Motorway => 27.8,
            Speed::Train => 69.4,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Speed::Walking => "Walking · 5 km/h",
            Speed::Running => "Running · 10 km/h",
            Speed::Cycling => "Cycling · 20 km/h",
            Speed::Town => "Driving in town · 50 km/h",
            Speed::Motorway => "Driving on a motorway · 100 km/h",
            Speed::Train => "By train · 250 km/h",
        }
    }
}

/// `route` is the share of the route already behind it, from 0 to 1, while
/// one plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Put {
    pub(super) latitude: f64,
    pub(super) longitude: f64,
    pub(super) route: Option<f32>,
}

#[derive(Clone, Copy)]
pub(crate) struct Routes {
    pub(super) put: RwSignal<Option<Put>>,
    /// Counts the routes started, so a route ends when another starts, the
    /// device is put somewhere, or it is stopped.
    plays: StoredValue<u32>,
    link: Link,
}

impl Routes {
    /// A route that plays ends when the picked device's owner is cleaned up,
    /// since its count goes with it.
    pub(crate) fn start(link: Link) -> Self {
        Routes {
            put: RwSignal::new(None),
            plays: StoredValue::new(0),
            link,
        }
    }

    fn playing(self, round: u32) -> bool {
        self.plays.try_get_value() == Some(round)
    }

    /// Returns the number of the next route that may start. None once another
    /// device was picked.
    pub(super) fn stop(self) -> Option<u32> {
        let round = self.plays.try_update_value(|round| {
            *round += 1;

            *round
        })?;

        self.put.update(|put| {
            if let Some(put) = put {
                put.route = None;
            }
        });

        Some(round)
    }

    pub(super) async fn follow(
        self,
        device: Rc<dyn Device>,
        id: DeviceId,
        route: Vec<(f64, f64)>,
        speed: Speed,
    ) {
        let Some(round) = self.stop() else {
            return;
        };

        let speed = speed.metres_a_second();
        let whole = length(&route);
        let mut gone = 0.0;

        loop {
            let (latitude, longitude) = along(&route, gone);
            let done = gone >= whole;
            let told = device.put_location(latitude, longitude).await;

            if !self.playing(round) {
                return;
            }

            if let Err(error) = told {
                self.link.did("location", Some(&id), Err(error));

                break;
            }

            let route = (!done).then(|| (gone / whole.max(1.0)) as f32);

            self.put.set(Some(Put {
                latitude,
                longitude,
                route,
            }));

            if done {
                break;
            }

            sleep(Duration::from_secs(1)).await;

            // Checked again after the wait, so a route that was stopped
            // meanwhile does not move the device once more.
            if !self.playing(round) {
                return;
            }

            gone = (gone + speed).min(whole);
        }

        self.stop();
    }
}
