use std::sync::Arc;

use protocol::{
    ArchivedToServer, ArchivedTopic, ClientId, Gather, ToClient, ToServer, Topic, UsbPhone, Value,
    access, encode, own,
};
use str0m::channel::ChannelId;
use tokio::sync::mpsc;

use crate::app::{App, Held};
use crate::rtc::{Peer, Said};
use crate::watch::look;
use crate::{media, relay};

struct Leaves(Arc<App>, String, Peer);

impl Drop for Leaves {
    fn drop(&mut self) {
        self.0.emulators.leave_camera(None, &self.1, false);
        self.0.topics.leave(self.2);

        if self.0.browsers.lock().unwrap().remove(&self.2).is_some() {
            self.0.reported.notify_waiters();

            let app = self.0.clone();

            tokio::spawn(async move { look(&app).await });
        }

        self.0.hub.leave(self.2);
    }
}

pub(crate) async fn converse(
    app: Arc<App>,
    client: Arc<protocol::Client>,
    peer: Peer,
    mut hearing: mpsc::UnboundedReceiver<Said>,
) {
    let hub = app.hub.clone();
    let _leaves = Leaves(app.clone(), client.print.clone(), peer);
    // Replies and topic changes, in the order they are to reach the client.
    // Once it has gone nothing receives them, which stops the tasks working for
    // it.
    let (out, mut outgoing) = mpsc::unbounded_channel::<Vec<u8>>();
    let mut channels = relay::Channels::new(app.clone(), hub.clone(), peer);
    let mut lanes = media::Lanes::new(app.clone(), client.clone(), hub.clone(), peer);
    let mut control = None;
    let mut gather = Gather::default();
    let send = |control: Option<ChannelId>, message: &[u8]| {
        if let Some(control) = control {
            for part in protocol::parts(message) {
                hub.write(peer, control, part, None);
            }
        }
    };

    app.topics.join(peer, &client.print, out.clone());

    loop {
        tokio::select! {
            said = hearing.recv() => {
                let Some(said) = said else {
                    return;
                };

                let message = match said {
                    Said::Opened(channel, label) => {
                        if label == protocol::CONTROL {
                            control = Some(channel);
                        } else if label == protocol::MEDIA {
                            lanes.open(channel);
                        } else if let Some((device, service)) = protocol::adb_of(&label) {
                            channels.open(channel, device, service);
                        } else {
                            hub.close(peer, channel);
                        }

                        continue;
                    }
                    Said::Data(channel, data) if Some(channel) == control => {
                        match gather.add(&data) {
                            Some(message) => message,
                            None => continue,
                        }
                    }
                    Said::Data(channel, data) => {
                        if !lanes.data(channel, &data) {
                            channels.write(channel, data);
                        }

                        continue;
                    }
                    // A page without its control channel has gone.
                    Said::Closed(channel) if Some(channel) == control => return,
                    Said::Closed(channel) => {
                        if !lanes.close(channel) {
                            channels.close(channel);
                        }

                        continue;
                    }
                    Said::KeyFrame(mid) => {
                        lanes.key_frame(mid);

                        continue;
                    }
                };

                if !heard(&app, &client, peer, &out, message).await {
                    return;
                }
            }
            Some(message) = outgoing.recv() => send(control, &message),
        }
    }
}

/// False if the message was invalid, which drops the client.
async fn heard(
    app: &Arc<App>,
    client: &Arc<protocol::Client>,
    peer: Peer,
    out: &mpsc::UnboundedSender<Vec<u8>>,
    message: Vec<u8>,
) -> bool {
    let Ok(asked) = access::<ToServer>(&message) else {
        tracing::warn!("A client was dropped: invalid message");
        return false;
    };

    match asked {
        ArchivedToServer::Call { id, .. } => {
            // Answered in a task of its own, because stopping an emulator
            // takes a while and the client should hear of other things
            // meanwhile.
            let id = id.to_native();
            let app = app.clone();
            let out = out.clone();
            let message = message.clone();
            let client = client.clone();

            tokio::spawn(async move {
                // Read again, because the task has to own the message it
                // reads from.
                let Ok(ArchivedToServer::Call { request, .. }) = access::<ToServer>(&message)
                else {
                    return;
                };

                let result = crate::answer::noted(&app, request, &client).await;

                let _ = out.send(encode(&ToClient::Reply { id, result }));
            });
        }
        ArchivedToServer::Subscribe(topic) => app.topics.subscribe(peer, own(topic)),
        ArchivedToServer::Unsubscribe(topic) => app.topics.unsubscribe(peer, &own(topic)),
        // Answered before the next message is read, so a change sent after a
        // subscribe that came first is never older than this answer.
        ArchivedToServer::Query(topic) => {
            if let Some(value) = query(app, topic).await {
                let _ = out.send(encode(&ToClient::Value(value)));
            }
        }
        ArchivedToServer::Event {
            what,
            failed,
            message,
            about,
        } => {
            let about = about.as_ref().map(own);

            app.journal
                .did(what, *failed, message, about.as_ref(), &client.name)
                .await;
        }
        ArchivedToServer::Offer { id, sdp } => {
            let id = id.to_native();
            let sdp = sdp.to_string();
            let app = app.clone();
            let out = out.clone();

            tokio::spawn(async move {
                let result = app
                    .hub
                    .renegotiate(peer, &sdp)
                    .await
                    .map_err(|error| format!("{error:#}"));

                let _ = out.send(encode(&ToClient::Reply { id, result }));
            });
        }
        ArchivedToServer::Usb(phones) => {
            let phones = own::<Vec<UsbPhone>>(phones);
            let held = Held {
                client: ClientId(client.print.clone()),
                phones,
            };

            app.browsers.lock().unwrap().insert(peer, held);
            app.reported.notify_waiters();

            let app = app.clone();

            tokio::spawn(async move { look(&app).await });
        }
    }

    true
}

async fn query(app: &App, topic: &ArchivedTopic) -> Option<Value> {
    Some(match own::<Topic>(topic) {
        Topic::Devices => Value::Devices(app.devices()),
        Topic::Access => Value::Access(app.trust.access()),
        Topic::Apps => Value::Apps(
            app.apps
                .list()
                .await
                .inspect_err(|error| tracing::error!("db failed: {error:#}"))
                .ok()?,
        ),
        Topic::Places => Value::Places(app.locations.places()),
        Topic::Events(_) => {
            let ArchivedTopic::Events(query) = topic else {
                return None;
            };

            Value::Events(
                app.journal
                    .page(query)
                    .await
                    .inspect_err(|error| tracing::error!("db failed: {error:#}"))
                    .ok()?,
            )
        }
        Topic::Emulator(name) => Value::Emulator {
            settings: app.emulators.settings(&name),
            name,
        },
    })
}
