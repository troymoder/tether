use std::time::Duration;

use leptos::prelude::*;

use super::Sources;
use crate::app::Picked;
use crate::ui::loadable::Loadable;
use crate::ui::poll::use_poll;

const EVERY: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum InFront {
    /// The device has no log to follow, so it is not asked.
    NotAsked,
    /// None on the device's home screen.
    App(Option<String>),
}

/// It is `Loading` until the device has answered once, and stays as it was
/// when asking fails.
pub(crate) fn use_front() -> Signal<Loadable<InFront>> {
    let device = expect_context::<Picked>().device;

    use_poll(EVERY, move || {
        let device = device.get_untracked();

        async move {
            if device.sources().contains(Sources::DEVICE) {
                device.front().await.map(InFront::App)
            } else {
                Ok(InFront::NotAsked)
            }
        }
    })
}

pub(crate) fn package(front: Signal<Loadable<InFront>>) -> Option<String> {
    match front.get() {
        Loadable::Ready(InFront::App(package)) => package,
        _ => None,
    }
}
