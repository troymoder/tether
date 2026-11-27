use futures_channel::oneshot;
use leptos::prelude::*;
use protocol::{
    App, ArchivedToClient, Device, DeviceId, EventPage, EventQuery, Media, Request, ToClient,
    ToServer, Topic, Value, access, encode, own,
};
use web_sys::RtcDataChannelState;

use super::state::seen_from;
use super::{Link, Reply, ServerStoreFields};

const ALWAYS: [Topic; 2] = [Topic::Devices, Topic::Access];

impl Link {
    pub(super) fn receive(self, message: &[u8]) {
        let Ok(message) = access::<ToClient>(message) else {
            return;
        };

        match message {
            ArchivedToClient::Reply { id, result } => {
                let waiting = self.connection(|connection| {
                    connection.waiting.borrow_mut().remove(&id.to_native())
                });

                if let Some(waiting) = waiting {
                    let _ = waiting.send(own::<Reply>(result));
                }
            }
            ArchivedToClient::Value(value) => self.apply(own::<Value>(value)),
            ArchivedToClient::Device(device) => {
                let you = self.you.get_untracked();
                let device = seen_from(own::<Device>(&**device), you.as_ref());

                // tetherd's update replaces whatever a pending action showed
                // (pending.rs).
                self.settle(Some(&device.id));
                self.put_device(device);
                self.keep();
            }
            ArchivedToClient::DeviceGone(id) => {
                let id = own::<DeviceId>(id);

                self.settle(Some(&id));
                self.server
                    .devices()
                    .write()
                    .retain(|device| device.id != id);
                self.keep();
            }
            ArchivedToClient::App(app) => self.put_app(own::<App>(app)),
            ArchivedToClient::AppGone(id) => {
                let id = id.to_string();

                self.server.apps().write().retain(|app| app.id != id);
            }
            ArchivedToClient::LetGo { device } => {
                let device = own::<DeviceId>(device);

                self.let_go(&device.serial());
            }
        }
    }

    fn apply(self, value: Value) {
        match value {
            Value::Devices(devices) => {
                let you = self.you.get_untracked();
                let devices = devices
                    .into_iter()
                    .map(|device| seen_from(device, you.as_ref()))
                    .collect();

                self.settle(None);
                self.server.devices().set(devices);
                self.keep();

                // Set only once, because app.rs rebuilds the open view
                // whenever this changes.
                if !self.loaded.get_untracked() {
                    self.loaded.set(true);
                }
            }
            Value::Access(access) => self.server.access().set(Some(access)),
            Value::Apps(apps) => self.server.apps().set(apps),
            Value::Places(places) => self.server.places().set(places),
            Value::Events(page) => {
                if let Some(heard) = self.connection(|connection| connection.events.get()) {
                    heard.run(page);
                }
            }
            Value::Emulator { name, settings } => self.put_settings(name, settings),
        }
    }

    /// Follows for as long as the calling view or effect lives. It reruns
    /// whenever a signal `topics` reads changes, so those are best memos.
    pub(crate) fn follow<T>(self, topics: impl Fn() -> T + 'static)
    where
        T: IntoIterator<Item = Topic>,
    {
        Effect::new(move |_| {
            let topics: Vec<Topic> = topics().into_iter().collect();

            for topic in &topics {
                self.want(topic.clone());
            }

            on_cleanup(move || {
                for topic in &topics {
                    self.unwant(topic);
                }
            });
        });
    }

    /// tetherd sends a new page whenever an event is stored, and for each new
    /// query.
    pub(crate) fn events(
        self,
        query: impl Fn() -> EventQuery + 'static,
        heard: Callback<EventPage>,
    ) {
        self.connection(|connection| connection.events.set(Some(heard)));
        on_cleanup(move || {
            self.connection
                .try_with_value(|connection| connection.events.set(None));
        });

        self.follow(move || Some(Topic::Events(query())));
    }

    fn want(self, topic: Topic) {
        let first = self.connection(|connection| {
            let mut topics = connection.topics.borrow_mut();

            match topics.iter_mut().find(|(known, _)| *known == topic) {
                Some((_, count)) => {
                    *count += 1;
                    false
                }
                None => {
                    topics.push((topic.clone(), 1));
                    true
                }
            }
        });

        if first {
            self.subscribe(topic);
        }
    }

    fn unwant(self, topic: &Topic) {
        let last = self
            .connection
            .try_with_value(|connection| {
                let mut topics = connection.topics.borrow_mut();
                let at = topics.iter().position(|(known, _)| known == topic)?;

                topics[at].1 -= 1;

                (topics[at].1 == 0).then(|| topics.remove(at))
            })
            .flatten();

        if last.is_some() {
            self.send(&ToServer::Unsubscribe(topic.clone()));
        }
    }

    /// Subscribes and then queries, in that order, so the answer holds every
    /// change sent before it.
    fn subscribe(self, topic: Topic) {
        self.send(&ToServer::Subscribe(topic.clone()));
        self.send(&ToServer::Query(topic));
    }

    pub(super) fn subscribed(self) {
        let wanted: Vec<Topic> = self.connection(|connection| {
            connection
                .topics
                .borrow()
                .iter()
                .map(|(topic, _)| topic.clone())
                .collect()
        });

        for topic in ALWAYS.into_iter().chain(wanted) {
            self.subscribe(topic);
        }
    }

    pub(crate) fn ask(self, request: Request) {
        leptos::task::spawn_local(async move {
            match self.call(request).await {
                Ok(said) | Err(said) => self.say(said),
            }
        });
    }

    /// Returns true if it was sent. Delivery is not confirmed, so anything
    /// that matters is sent again after a reconnect.
    pub(crate) fn send(self, message: &ToServer) -> bool {
        let message = encode(message);
        self.connection(|connection| {
            connection
                .peer
                .borrow()
                .as_ref()
                .map(|peer| &peer.control)
                .filter(|control| control.ready_state() == RtcDataChannelState::Open)
                .is_some_and(|control| {
                    protocol::parts(&message).all(|part| control.send_with_u8_array(&part).is_ok())
                })
        })
    }

    async fn asked(self, request: Request) -> Result<Reply, String> {
        self.replied(|id| ToServer::Call { id, request }).await
    }

    pub(crate) async fn offer(self, sdp: String) -> Reply {
        self.replied(|id| ToServer::Offer { id, sdp }).await?
    }

    async fn replied(self, message: impl FnOnce(u32) -> ToServer) -> Result<Reply, String> {
        let (answer, answered) = oneshot::channel();

        let sent = self.connection(|connection| {
            let id = connection.calls.get();

            connection.calls.set(id.wrapping_add(1));
            connection.waiting.borrow_mut().insert(id, answer);

            let sent = self.send(&message(id));

            if !sent {
                connection.waiting.borrow_mut().remove(&id);
            }

            sent
        });

        if !sent {
            return Err("not connected to the server".to_owned());
        }

        answered
            .await
            .map_err(|_| "connection to the server lost".to_owned())
    }

    pub(crate) async fn call(self, request: Request) -> Reply {
        self.asked(request).await?
    }

    /// For a request too large for the control channel, such as one carrying
    /// an app's package.
    pub(crate) async fn call_large(self, request: Request) -> Reply {
        let Some(lane) = self.lane(&Media::Call { request }) else {
            return Err("not connected to the server".to_owned());
        };

        let Some(answer) = lane.next().await else {
            return Err("connection to the server lost".to_owned());
        };

        access::<Reply>(&answer)
            .map(own::<Reply>)
            .map_err(|error| format!("call failed: {error}"))?
    }
}
