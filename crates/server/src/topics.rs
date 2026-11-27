use std::collections::HashMap;
use std::sync::Mutex;

use protocol::{EventQuery, ToClient, Topic, Value, encode};
use tokio::sync::mpsc;

use crate::rtc::Peer;

#[derive(Default)]
pub(crate) struct Topics(Mutex<HashMap<Peer, Subscriber>>);

struct Subscriber {
    print: String,
    /// Its control channel, which carries the answers to its queries too, so
    /// changes and answers reach the client in the order they were sent.
    out: mpsc::UnboundedSender<Vec<u8>>,
    topics: Vec<Topic>,
}

/// A client has one events query at a time, so two events topics are one
/// subscription whatever their queries.
fn same(one: &Topic, other: &Topic) -> bool {
    matches!((one, other), (Topic::Events(_), Topic::Events(_))) || one == other
}

impl Topics {
    pub(crate) fn join(&self, peer: Peer, print: &str, out: mpsc::UnboundedSender<Vec<u8>>) {
        let subscriber = Subscriber {
            print: print.to_owned(),
            out,
            topics: Vec::new(),
        };

        self.0.lock().unwrap().insert(peer, subscriber);
    }

    pub(crate) fn leave(&self, peer: Peer) {
        self.0.lock().unwrap().remove(&peer);
    }

    pub(crate) fn subscribe(&self, peer: Peer, topic: Topic) {
        if let Some(subscriber) = self.0.lock().unwrap().get_mut(&peer) {
            subscriber.topics.retain(|kept| !same(kept, &topic));
            subscriber.topics.push(topic);
        }
    }

    pub(crate) fn unsubscribe(&self, peer: Peer, topic: &Topic) {
        if let Some(subscriber) = self.0.lock().unwrap().get_mut(&peer) {
            subscriber.topics.retain(|kept| !same(kept, topic));
        }
    }

    pub(crate) fn tell(&self, peer: Peer, message: &ToClient) {
        if let Some(subscriber) = self.0.lock().unwrap().get(&peer) {
            let _ = subscriber.out.send(encode(message));
        }
    }

    pub(crate) fn send(&self, wants: impl Fn(&Topic) -> bool, message: &ToClient) {
        let mut encoded = None;

        for subscriber in self.0.lock().unwrap().values() {
            if subscriber.topics.iter().any(&wants) {
                let bytes = encoded.get_or_insert_with(|| encode(message));

                let _ = subscriber.out.send(bytes.clone());
            }
        }
    }

    pub(crate) fn value(&self, topic: &Topic, value: Value) {
        self.send(|kept| kept == topic, &ToClient::Value(value));
    }

    pub(crate) fn peers(&self, print: &str) -> Vec<Peer> {
        let subscribers = self.0.lock().unwrap();

        subscribers
            .iter()
            .filter(|(_, subscriber)| subscriber.print == print)
            .map(|(peer, _)| *peer)
            .collect()
    }

    pub(crate) fn event_queries(&self) -> Vec<(EventQuery, mpsc::UnboundedSender<Vec<u8>>)> {
        let subscribers = self.0.lock().unwrap();

        subscribers
            .values()
            .flat_map(|subscriber| {
                subscriber.topics.iter().filter_map(|topic| match topic {
                    Topic::Events(query) => Some((query.clone(), subscriber.out.clone())),
                    _ => None,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use protocol::{ArchivedToClient, access};

    use super::*;

    #[test]
    fn sends_a_change_only_to_the_subscribers_of_its_topic() {
        let topics = Topics::default();
        let (devices, mut to_devices) = mpsc::unbounded_channel();
        let (events, mut to_events) = mpsc::unbounded_channel();

        topics.join(1, "SHA256:a", devices);
        topics.join(2, "SHA256:b", events);
        topics.subscribe(1, Topic::Devices);
        topics.subscribe(2, Topic::Events(EventQuery::default()));

        topics.subscribe(
            2,
            Topic::Events(EventQuery {
                most: 5,
                ..EventQuery::default()
            }),
        );

        assert_eq!(topics.event_queries().len(), 1);
        assert_eq!(topics.event_queries()[0].0.most, 5);

        topics.send(
            |topic| *topic == Topic::Devices,
            &ToClient::AppGone("x".to_owned()),
        );

        let sent = to_devices.try_recv().unwrap();

        assert!(matches!(
            access::<ToClient>(&sent),
            Ok(ArchivedToClient::AppGone(_))
        ));
        assert!(to_events.try_recv().is_err());

        topics.unsubscribe(2, &Topic::Events(EventQuery::default()));
        assert!(topics.event_queries().is_empty());
        assert_eq!(topics.peers("SHA256:b"), [2]);

        topics.leave(2);
        assert!(topics.peers("SHA256:b").is_empty());
    }
}
