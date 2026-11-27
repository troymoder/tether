//! One WebRTC connection, signalled over HTTP, carries everything and is
//! rebuilt whenever it drops.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use futures_channel::oneshot;
use leptos::prelude::*;
use protocol::{ClientId, DeviceId, EventPage, Gate, Topic, UsbPhone};
use reactive_stores::Store;

pub(crate) mod capture;
mod connection;
mod devices;
mod files;
pub(crate) mod heard;
mod ios;
mod key;
pub(crate) mod lane;
mod messages;
mod notices;
mod pending;
pub(crate) mod relay;
mod state;
mod stats;
pub(crate) mod tracks;
pub(crate) mod usb;
pub(crate) mod worker;

pub(crate) use files::{address, read, save};
pub(crate) use notices::unimplemented;
pub(crate) use state::{Server, ServerStoreFields, means};
pub(crate) use stats::{Figures, MINUTE};

#[derive(Clone, Copy)]
pub(crate) struct Link {
    pub server: Store<Server>,
    pub usb: RwSignal<Vec<UsbPhone>>,
    pub linked: RwSignal<Linked>,
    /// True while the control channel is open, interrupted or not. A memo, so
    /// its readers do not rerun each time the route changes.
    pub connected: Memo<bool>,
    pub you: RwSignal<Option<ClientId>>,
    /// Separate from `linked` because it outlives reconnects, which keeps the
    /// views open.
    pub loaded: RwSignal<bool>,
    pub figures: Figures,
    connection: StoredValue<Connection, LocalStorage>,
    picked: Signal<Option<DeviceId>>,
    notify: Callback<String>,
    phones: StoredValue<usb::Phones, LocalStorage>,
    pending: RwSignal<pending::Pending>,
}

type Reply = Result<String, String>;

#[derive(Clone, PartialEq)]
pub(crate) enum Linked {
    Connecting {
        retrying: bool,
    },
    Gate(Gate),
    Connected {
        /// In milliseconds since the epoch.
        since: f64,
        route: Option<String>,
        interrupted: bool,
    },
}

impl Linked {
    pub(crate) fn lost(&self) -> bool {
        match self {
            Linked::Connecting { retrying } => *retrying,
            Linked::Gate(_) => false,
            Linked::Connected { interrupted, .. } => *interrupted,
        }
    }

    pub(crate) fn up(&self) -> bool {
        matches!(
            self,
            Linked::Connected {
                interrupted: false,
                ..
            }
        )
    }
}

#[derive(Default)]
struct Connection {
    /// Counts the attempts at connecting, so work left from an earlier
    /// attempt is dropped once a newer one has begun.
    tries: Cell<u32>,
    /// The attempts that failed in a row, which sets the delay before the next.
    failed: Cell<u32>,
    peer: RefCell<Option<connection::Peer>>,
    code: RefCell<Option<String>>,
    /// True once the user was told that Firefox can't connect here.
    told: Cell<bool>,
    waiting: RefCell<HashMap<u32, oneshot::Sender<Reply>>>,
    calls: Cell<u32>,
    relays: relay::Relays,
    /// True while an offer is out, because a second offer before its answer
    /// would fail (tracks.rs).
    offering: Cell<bool>,
    /// Each topic with the number of views that follow it, subscribed to
    /// again on each new control channel.
    topics: RefCell<Vec<(Topic, u32)>>,
    events: Cell<Option<Callback<EventPage>>>,
}

impl Link {
    pub(crate) fn start(notify: Callback<String>, picked: Signal<Option<DeviceId>>) -> Self {
        let linked = RwSignal::new(Linked::Connecting { retrying: false });
        let link = Self {
            server: Store::new(Server::default()),
            usb: RwSignal::new(Vec::new()),
            linked,
            connected: Memo::new(move |_| {
                linked.with(|linked| matches!(linked, Linked::Connected { .. }))
            }),
            you: RwSignal::new(None),
            loaded: RwSignal::new(false),
            figures: Figures::new(),
            connection: StoredValue::new_local(Connection::default()),
            picked,
            notify,
            phones: StoredValue::new_local(Default::default()),
            pending: RwSignal::new(HashMap::new()),
        };

        link.connect();
        link.watch_usb();
        link.measure();

        link
    }

    fn connection<T>(self, then: impl FnOnce(&Connection) -> T) -> T {
        self.connection.with_value(then)
    }

    /// Notifies readers only if the state changed. Notifying app/gate.rs of
    /// the same gate would restart its form.
    pub(crate) fn relink(self, change: impl FnOnce(&mut Linked)) {
        self.linked.maybe_update(|linked| {
            let before = linked.clone();

            change(linked);

            *linked != before
        });
    }

    pub(crate) fn relays<T>(self, then: impl FnOnce(&relay::Relays) -> T) -> T {
        self.connection(|connection| then(&connection.relays))
    }
}
