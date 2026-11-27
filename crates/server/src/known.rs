use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use protocol::{DeviceId, Last, Way, mention};

use crate::db::{Db, Known};

pub(crate) struct Devices {
    db: Arc<Db>,
    known: Mutex<BTreeMap<DeviceId, Known>>,
}

impl Devices {
    pub(crate) async fn new(db: Arc<Db>) -> Result<Self> {
        Ok(Self {
            known: Mutex::new(db.devices().await?),
            db,
        })
    }

    pub(crate) fn all(&self) -> BTreeMap<DeviceId, Known> {
        self.known.lock().unwrap().clone()
    }

    pub(crate) async fn connected(&self, id: &DeviceId, name: &str, way: &Way) -> Last {
        let last = Last {
            way: way.clone(),
            at: now(),
        };
        let _writing = self.db.writing().await;
        let kept = {
            let mut known = self.known.lock().unwrap();
            let entry = known.entry(id.clone()).or_insert_with(|| Known {
                name: name.to_owned(),
                given: None,
                last: None,
            });

            entry.last = Some(last.clone());
            entry.clone()
        };

        if let Err(error) = self.db.save_device(id, &kept).await {
            tracing::error!("db failed: {id}: {error:#}");
        }

        last
    }

    pub(crate) fn name(&self, id: &DeviceId) -> Option<String> {
        let known = self.known.lock().unwrap();

        known.get(id).map(|known| called(known).to_owned())
    }

    /// `called` is the device's current name, kept as its own when it is not
    /// known yet.
    pub(crate) async fn rename(&self, id: &DeviceId, called: &str, name: &str) -> Result<String> {
        let _writing = self.db.writing().await;
        let (was, kept) = {
            let mut known = self.known.lock().unwrap();
            let entry = known.entry(id.clone()).or_insert_with(|| Known {
                name: called.to_owned(),
                given: None,
                last: None,
            });
            let was = self::called(entry).to_owned();

            entry.given = Some(name.to_owned());

            (was, entry.clone())
        };

        self.db.save_device(id, &kept).await?;

        // The old name in words, since a mention would show the new one.
        Ok(format!("Renamed {was} to {name}"))
    }

    pub(crate) async fn forget(&self, id: &DeviceId) -> Result<String> {
        let _writing = self.db.writing().await;
        let name = self
            .name(id)
            .ok_or_else(|| anyhow::anyhow!("forget failed: {} not known", mention(id)))?;

        self.db.forget_device(id).await?;
        self.known.lock().unwrap().remove(id);

        Ok(format!("Forgot {name}"))
    }
}

pub(crate) fn called(known: &Known) -> &str {
    known.given.as_deref().unwrap_or(&known.name)
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use protocol::{At, InstanceId, Means};

    use super::*;

    #[tokio::test]
    async fn a_device_is_known_once_it_has_connected() {
        let db = Arc::new(Db::in_memory().await);
        let devices = Devices::new(db.clone()).await.unwrap();
        let id = DeviceId::Android("AAA".to_owned());
        let way = |means| Way {
            means,
            at: At::Instance(InstanceId("SHA256:here".to_owned())),
        };

        assert_eq!(devices.name(&id), None);
        devices.connected(&id, "Phone", &way(Means::Usb)).await;
        assert_eq!(devices.name(&id).as_deref(), Some("Phone"));

        devices.connected(&id, "Other", &way(Means::Wifi)).await;
        let kept = &db.devices().await.unwrap()[&id];

        assert_eq!(kept.name, "Phone");
        assert_eq!(kept.last.as_ref().unwrap().way, way(Means::Wifi));

        devices.rename(&id, "Phone", "Desk phone").await.unwrap();
        let devices = Devices::new(db.clone()).await.unwrap();

        assert_eq!(devices.name(&id).as_deref(), Some("Desk phone"));

        assert_eq!(devices.forget(&id).await.unwrap(), "Forgot Desk phone");
        assert!(db.devices().await.unwrap().is_empty());
        assert!(devices.forget(&id).await.is_err());
    }
}
