//! Two parts of the page are about the picked device: the open view and the
//! device's pane. They cannot share one keyed parent, since the views that
//! need no device are in the same place and must stay when a device is
//! picked. So each part is a keyed child of its own (app.rs), and both are
//! given the same [`Picked`].
//!
//! Each pick's services are made under an owner of its own, which `Alive`
//! cleans up when its last holder lets go. The holders are the memo of
//! [`use_picked`] and the `<Provider>` of each part. A part that is still up
//! therefore reads services that are whole, and the old device's services go
//! as soon as nothing shows them.
//!
//! The owner is a child of the page's. The memo's own owner is cleaned up
//! each time the memo runs again, which would dispose of the old services
//! while a part still showed them.
//!
//! Cleaning the pick's owner up from inside another owner's teardown is
//! sound in reactive_graph 0.2. `Owner::cleanup` drops the arena's values
//! after releasing the arena, and no lock is held at any place the guard is
//! dropped.
//!
//! A task or a browser callback that outlives the pick finds the handles
//! disposed, so it reads them with the `try_` methods.

use std::rc::Rc;
use std::sync::Arc;

use leptos::prelude::*;
use protocol::{Condition, DeviceId, Status};

use super::Global;
use crate::device::{self, Device, Views};
use crate::link::Link;
use crate::link::capture::Feed;
use crate::ui;
use crate::ui::button::{Button, Tone};
use crate::ui::pane::Pane;
use crate::views::clipboard::Board;
use crate::views::location::Routes;
use crate::views::screen::Control;
use crate::views::shell::Console;

#[derive(Clone)]
pub(crate) struct Picked {
    pub id: DeviceId,
    /// A signal, because a device that cannot be reached answers through
    /// another type.
    pub device: Signal<Rc<dyn Device>, LocalStorage>,
    pub listed: Signal<protocol::Device>,
    pub board: Board,
    pub control: Control,
    pub console: Console,
    pub feed: Feed,
    pub routes: Routes,
    alive: Arc<Alive>,
}

/// Dropping it cleans up the owner a pick's services were made under.
struct Alive(Owner);

impl Drop for Alive {
    fn drop(&mut self) {
        self.0.cleanup();
    }
}

impl PartialEq for Picked {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.alive, &other.alive)
    }
}

impl Picked {
    fn start(page: &Owner, link: Link, id: DeviceId) -> Option<Self> {
        let first = link.device(&id)?;
        let alive = Arc::new(Alive(page.child()));

        Some(alive.0.with(|| {
            let of = id.clone();
            let asked = id.clone();
            // The last listing is kept for the moment between tetherd
            // taking the device off its list and the page moving on.
            let listed = Memo::new(move |last: Option<&protocol::Device>| {
                let last = last.cloned().unwrap_or_else(|| first.clone());

                link.device(&of).unwrap_or(last)
            });
            let device = Signal::derive_local(move || device::of(link, asked.clone()));
            let control = Control::new();
            let feed = Feed::start(link, &id);
            let board = Board::start(link, control, device, &id);

            Picked {
                id,
                device,
                listed: listed.into(),
                board,
                control,
                console: Console::new(),
                feed,
                routes: Routes::start(link),
                alive: alive.clone(),
            }
        }))
    }
}

pub(super) fn use_picked() -> Memo<Option<Picked>> {
    let Global { link, router, .. } = expect_context::<Global>();
    let page = Owner::current().expect("the page has an owner");

    Memo::new(move |_| {
        let id = link.loaded.get().then(|| router.device()).flatten()?;

        // Only the id makes the services again, whatever starting them
        // reads.
        untrack(|| Picked::start(&page, link, id))
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Unavailable {
    NotRunning { startable: bool },
    NotConnected { connectable: bool },
    Lacking,
}

fn unavailable(listed: &protocol::Device, offered: Views, needed: Views) -> Option<Unavailable> {
    if offered.contains(needed) {
        return None;
    }

    Some(match &listed.emulator {
        _ if listed.ready() => Unavailable::Lacking,
        Some(run) => Unavailable::NotRunning {
            startable: run.status == Status::Shutdown,
        },
        None => Unavailable::NotConnected {
            connectable: listed.connected().is_none() && listed.condition != Condition::Unavailable,
        },
    })
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Needs(view: Views, title: &'static str, children: ChildrenFn) -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let Picked {
        id, device, listed, ..
    } = expect_context::<Picked>();
    let id = StoredValue::new(id);
    let why = Memo::new(move |_| {
        listed.with(|listed| unavailable(listed, device.get().views().get(), view))
    });
    let name = Memo::new(move |_| listed.with(|listed| listed.name.clone()));

    move || {
        let Some(why) = why.get() else {
            return children();
        };

        let name = name.get();
        let line = match why {
            Unavailable::NotRunning { .. } => format!("{name} is not running."),
            Unavailable::NotConnected { .. } => format!("{name} is not connected."),
            Unavailable::Lacking => format!("{name} has no {title}."),
        };
        let act = match why {
            Unavailable::NotRunning { startable: true } => Some("Start"),
            Unavailable::NotConnected { connectable: true } => Some("Connect"),
            _ => None,
        };

        view! {
            <Pane title>
                <div class=ui::style::pane_body>
                    <p class=ui::style::hint>{line}</p>
                    {act
                        .map(|act| {
                            view! {
                                <Button
                                    tone=Tone::Blue
                                    on:click=move |_| link.connect_to(ui, id.get_value(), None)
                                >
                                    {act}
                                </Button>
                            }
                        })}
                </div>
            </Pane>
        }
        .into_any()
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn NoDevice(title: &'static str) -> impl IntoView {
    view! {
        <Pane title>
            <div class=ui::style::pane_body>
                <p class=ui::style::hint>"No device."</p>
            </div>
        </Pane>
    }
}

#[cfg(test)]
mod tests {
    use protocol::{At, DeviceState, InstanceId, Means, Run, Way};

    use super::*;

    fn phone(condition: Condition) -> protocol::Device {
        protocol::Device {
            id: DeviceId::Android("R58M".to_owned()),
            name: "Pixel".to_owned(),
            condition,
            routes: Vec::new(),
            last: None,
            emulator: None,
        }
    }

    fn connected(state: DeviceState) -> Condition {
        Condition::Connected {
            way: Way {
                means: Means::Usb,
                at: At::Instance(InstanceId("SHA256:primary".to_owned())),
            },
            state,
            described: None,
        }
    }

    fn emulator(status: Status, condition: Condition) -> protocol::Device {
        protocol::Device {
            emulator: Some(Run {
                avd: "Pixel_9".to_owned(),
                status,
                upright: true,
                camera: None,
                fed_by: None,
                remote: None,
            }),
            ..phone(condition)
        }
    }

    #[test]
    fn a_view_the_device_offers_is_shown() {
        let ready = phone(connected(DeviceState::Ready));
        let stopped = emulator(Status::Shutdown, Condition::Available);

        assert_eq!(unavailable(&ready, Views::all(), Views::FILES), None);
        // A shut-down emulator still offers its log.
        assert_eq!(unavailable(&stopped, Views::LOGS, Views::LOGS), None);
    }

    #[test]
    fn a_device_that_does_not_answer_says_how_to_reach_it() {
        let none = Views::empty();
        let not_connected = |connectable| Some(Unavailable::NotConnected { connectable });
        let not_running = |startable| Some(Unavailable::NotRunning { startable });

        assert_eq!(
            unavailable(&phone(Condition::Available), none, Views::FILES),
            not_connected(true)
        );
        assert_eq!(
            unavailable(&phone(Condition::InUse), none, Views::FILES),
            not_connected(true)
        );
        // Nothing connects a phone tetherd sees no way to, or one that is
        // connected already and has yet to answer.
        assert_eq!(
            unavailable(&phone(Condition::Unavailable), none, Views::FILES),
            not_connected(false)
        );
        assert_eq!(
            unavailable(
                &phone(connected(DeviceState::Unauthorized)),
                none,
                Views::FILES
            ),
            not_connected(false)
        );
        assert_eq!(
            unavailable(
                &emulator(Status::Shutdown, Condition::Available),
                Views::LOGS,
                Views::FILES
            ),
            not_running(true)
        );
        assert_eq!(
            unavailable(
                &emulator(Status::Booting, Condition::Available),
                Views::LOGS,
                Views::FILES
            ),
            not_running(false)
        );
    }

    #[test]
    fn a_device_that_answers_lacks_the_view() {
        let iphone = phone(connected(DeviceState::Ready));
        let booted = emulator(Status::Booted, connected(DeviceState::Ready));

        assert_eq!(
            unavailable(&iphone, Views::APPS, Views::SHELL),
            Some(Unavailable::Lacking)
        );
        assert_eq!(
            unavailable(&booted, Views::APPS, Views::SHELL),
            Some(Unavailable::Lacking)
        );
    }
}
