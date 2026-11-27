//! The device is shown as the action will leave it until tetherd's next
//! update on it, which replaces it whatever it says. A call that fails puts
//! the device back as it was, and so does no update within `PATIENCE`. While
//! one action is under way the device takes no other.

use std::collections::HashMap;
use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use protocol::{Condition, Device, DeviceId, DeviceState, Request, Status, Way};

use super::Link;

/// tetherd answers a start or a connect at once and reports the device as
/// booting or connecting right after, so this only runs out when something is
/// wrong.
const PATIENCE: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Action {
    Start,
    Stop,
    Connect(Way),
    Disconnect,
}

pub(super) type Pending = HashMap<DeviceId, Underway>;

/// `asked` is when it was asked, which tells it apart from a later action.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Underway {
    action: Action,
    before: Device,
    asked: f64,
}

impl Action {
    fn after(&self, device: &Device) -> Device {
        let mut after = device.clone();

        match self {
            Action::Start | Action::Stop => {
                if let Some(run) = &mut after.emulator {
                    run.status = if *self == Action::Start {
                        Status::Booting
                    } else {
                        Status::Stopping
                    };
                }
            }
            Action::Connect(way) => {
                after.condition = Condition::Connected {
                    way: way.clone(),
                    state: DeviceState::Connecting,
                    described: None,
                };
            }
            // A device with a route left can be connected again.
            Action::Disconnect => {
                after.condition = if after.routes.is_empty() {
                    Condition::Unavailable
                } else {
                    Condition::Available
                };
            }
        }

        after
    }

    fn unmet(&self, name: &str) -> String {
        match self {
            Action::Start => format!("start failed: {name} did not start"),
            Action::Stop => format!("stop failed: {name} did not stop"),
            Action::Connect(_) => format!("connect failed: {name} did not connect"),
            Action::Disconnect => format!("disconnect failed: {name} did not disconnect"),
        }
    }
}

impl Link {
    pub(crate) fn busy(self, id: &DeviceId) -> bool {
        self.pending.with(|pending| pending.contains_key(id))
    }

    pub(crate) fn boot(self, id: DeviceId) {
        if let Some(name) = untrack(|| self.run(&id)).map(|run| run.avd) {
            self.act(id, Action::Start, Request::Start { name });
        }
    }

    pub(crate) fn shut_down(self, id: DeviceId) {
        if let Some(name) = untrack(|| self.run(&id)).map(|run| run.avd) {
            self.act(id, Action::Stop, Request::Stop { name });
        }
    }

    pub(super) fn act(self, id: DeviceId, action: Action, request: Request) {
        if self
            .pending
            .with_untracked(|pending| pending.contains_key(&id))
        {
            return;
        }

        let Some(before) = untrack(|| self.device(&id)) else {
            return;
        };

        let asked = js_sys::Date::now();
        let after = action.after(&before);
        let unanswered = id.clone();
        let unmet = action.clone();

        self.pending.update(|pending| {
            pending.insert(
                id.clone(),
                Underway {
                    action,
                    before,
                    asked,
                },
            );
        });
        self.put_device(after);

        set_timeout(
            move || {
                if let Some(before) = self.undo(&unanswered, asked) {
                    self.say(unmet.unmet(&before.name));
                }
            },
            PATIENCE,
        );

        spawn_local(async move {
            match self.call(request).await {
                Ok(said) => self.say(said),
                Err(said) => {
                    self.undo(&id, asked);
                    self.say(said);
                }
            }
        });
    }

    fn undo(self, id: &DeviceId, asked: f64) -> Option<Device> {
        let underway = self
            .pending
            .try_update(|pending| {
                pending
                    .get(id)
                    .is_some_and(|underway| underway.asked == asked)
                    .then(|| pending.remove(id))
                    .flatten()
            })
            .flatten()?;

        self.put_device(underway.before.clone());

        Some(underway.before)
    }

    /// None for every device, as when tetherd resends them all.
    pub(super) fn settle(self, id: Option<&DeviceId>) {
        self.pending.maybe_update(|pending| match id {
            Some(id) => pending.remove(id).is_some(),
            None => {
                let any = !pending.is_empty();

                pending.clear();
                any
            }
        });
    }
}
