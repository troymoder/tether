use std::sync::{Arc, Mutex};

use anyhow::Result;
use protocol::{
    ArchivedEventQuery, ArchivedRequest, DeviceId, Event, EventPage, EventQuery, ToClient, Value,
    access, encode,
};

use crate::db::Db;
use crate::known::now;
use crate::topics::Topics;

pub(crate) const KEPT: usize = 10_000;

pub(crate) struct Journal {
    db: Arc<Db>,
    topics: Arc<Topics>,
    host: String,
    newest: Mutex<Option<Event>>,
}

impl Journal {
    pub(crate) async fn new(db: Arc<Db>, topics: Arc<Topics>) -> Result<Self> {
        let host = crate::trust::host_name();

        Ok(Self {
            newest: Mutex::new(db.newest_event().await?),
            host,
            db,
            topics,
        })
    }

    pub(crate) async fn page(&self, query: &ArchivedEventQuery) -> Result<EventPage> {
        self.db.events(query).await
    }

    pub(crate) async fn answered(
        &self,
        request: &ArchivedRequest,
        result: &Result<String, String>,
        about: Option<&DeviceId>,
    ) {
        let Some(what) = kind(request) else {
            return;
        };

        let (failed, message) = match result {
            Ok(said) => (false, said),
            Err(said) => (true, said),
        };
        let place = self.host.clone();

        self.add(what, failed, message, about, place).await;
    }

    pub(crate) async fn done(
        &self,
        what: &str,
        result: &Result<String, String>,
        about: Option<&DeviceId>,
    ) {
        let (failed, message) = match result {
            Ok(said) => (false, said),
            Err(said) => (true, said),
        };
        let place = self.host.clone();

        self.add(what, failed, message, about, place).await;
    }

    pub(crate) async fn did(
        &self,
        what: &str,
        failed: bool,
        message: &str,
        about: Option<&DeviceId>,
        client: &str,
    ) {
        self.add(what, failed, message, about, client.to_owned())
            .await;
    }

    /// A repeat of the newest event is dropped, because a request retried while
    /// it keeps failing would otherwise fill the list.
    async fn add(
        &self,
        what: &str,
        failed: bool,
        message: &str,
        device: Option<&DeviceId>,
        place: String,
    ) {
        let _writing = self.db.writing().await;
        let again = self.newest.lock().unwrap().as_ref().is_some_and(|newest| {
            newest.message == message
                && newest.device.as_ref() == device
                && newest.failed == failed
                && newest.place == place
        });

        if again {
            return;
        }

        let mut event = Event {
            id: 0,
            at: now(),
            failed,
            what: what.chars().take(40).collect(),
            message: message.to_owned(),
            device: device.cloned(),
            place,
        };

        match self.db.add_event(&event, KEPT).await {
            Ok(id) => {
                event.id = id;
                *self.newest.lock().unwrap() = Some(event);
            }
            Err(error) => {
                tracing::error!("db failed: {error:#}");
                return;
            }
        }

        for (query, out) in self.topics.event_queries() {
            let db = self.db.clone();

            tokio::spawn(async move {
                match page(&db, &query).await {
                    Ok(page) => {
                        let _ = out.send(encode(&ToClient::Value(Value::Events(page))));
                    }
                    Err(error) => tracing::error!("db failed: {error:#}"),
                }
            });
        }
    }
}

async fn page(db: &Db, query: &EventQuery) -> Result<EventPage> {
    let bytes = encode(query);
    let query = access::<EventQuery>(&bytes).map_err(anyhow::Error::msg)?;

    db.events(query).await
}

/// None for requests that only read or that come many times a second, which are
/// not recorded.
fn kind(request: &ArchivedRequest) -> Option<&'static str> {
    Some(match request {
        ArchivedRequest::Approve { .. }
        | ArchivedRequest::Reject { .. }
        | ArchivedRequest::Revoke { .. }
        | ArchivedRequest::Rename { .. }
        | ArchivedRequest::RenameInstance { .. } => "access",
        ArchivedRequest::RenameDevice { .. } => "rename",
        ArchivedRequest::ForgetDevice { .. } => "forget",
        ArchivedRequest::CreateEmulator { .. } => "create",
        ArchivedRequest::Configure { .. } => "configure",
        ArchivedRequest::Delete { .. } => "delete",
        ArchivedRequest::Erase { .. } => "erase",
        ArchivedRequest::Start { .. } => "boot",
        ArchivedRequest::Stop { .. } => "stop",
        ArchivedRequest::Connect { code, .. } | ArchivedRequest::Reach { code, .. }
            if code.is_some() =>
        {
            "pair"
        }
        ArchivedRequest::Connect { .. } | ArchivedRequest::Reach { .. } => "connect",
        ArchivedRequest::Disconnect { .. } => "disconnect",
        ArchivedRequest::HandOver { .. } | ArchivedRequest::AddApp { .. } => "add app",
        ArchivedRequest::InstallApp { .. } => "install",
        ArchivedRequest::ForgetApp { .. } => "forget",
        ArchivedRequest::AutoInstall { .. } => "apps",
        ArchivedRequest::TakeCamera { .. } | ArchivedRequest::StopRemote { .. } => "camera",
        // GeoFix is left out because a route sends it every second. Each
        // variant is named so that a new request needs a decision here.
        ArchivedRequest::EmulatorLog { .. }
        | ArchivedRequest::GeoFix { .. }
        | ArchivedRequest::KeepPlace(_)
        | ArchivedRequest::ForgetPlace { .. }
        | ArchivedRequest::AppIcon { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use protocol::{ArchivedToClient, Topic};
    use tokio::sync::mpsc;

    use super::*;

    #[tokio::test]
    async fn keeps_events_and_sends_them_to_the_subscribers() {
        let db = Arc::new(Db::in_memory().await);
        let topics = Arc::new(Topics::default());
        let (out, mut sent) = mpsc::unbounded_channel();

        topics.join(1, "SHA256:a", out);
        topics.subscribe(
            1,
            Topic::Events(EventQuery {
                most: 1,
                ..EventQuery::default()
            }),
        );

        let journal = Journal::new(db.clone(), topics).await.unwrap();
        let pixel = DeviceId::Android("pixel".to_owned());
        let did =
            |what, failed, message| journal.did(what, failed, message, Some(&pixel), "Firefox");

        did("files", true, "adb failed: gone").await;

        let told = sent.recv().await.unwrap();

        assert!(matches!(
            access::<ToClient>(&told),
            Ok(ArchivedToClient::Value(_))
        ));

        did("files", true, "adb failed: gone").await;
        did("apps", false, "Installed").await;
        did("apps", true, "adb failed: other").await;

        let again = Journal::new(db.clone(), Arc::default()).await.unwrap();

        assert_eq!(
            again.newest.lock().unwrap().as_ref().map(|event| event.id),
            Some(3)
        );

        let messages = |page: &EventPage| {
            page.events
                .iter()
                .map(|event| event.message.clone())
                .collect::<Vec<_>>()
        };
        let all = page(
            &db,
            &EventQuery {
                most: 10,
                ..EventQuery::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(
            messages(&all),
            ["adb failed: other", "Installed", "adb failed: gone"]
        );
        assert_eq!(all.whats, ["apps", "files"]);
        assert_eq!(all.devices, std::slice::from_ref(&pixel));
        assert_eq!(all.places, ["Firefox"]);
        assert!(!all.more);

        let failed = page(
            &db,
            &EventQuery {
                failed: Some(true),
                search: Some("ADB".to_owned()),
                device: Some(pixel.clone()),
                oldest_first: true,
                most: 1,
                ..EventQuery::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(messages(&failed), ["adb failed: gone"]);
        assert!(failed.more);

        // `%` is matched literally.
        let none = page(
            &db,
            &EventQuery {
                search: Some("%".to_owned()),
                most: 10,
                ..EventQuery::default()
            },
        )
        .await
        .unwrap();

        assert!(none.events.is_empty());
    }
}
