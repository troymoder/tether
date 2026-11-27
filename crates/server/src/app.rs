use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use protocol::{
    At, ClientId, Device, DeviceId, Means, Route, ToClient, Topic, UsbPhone, Way, mention,
};
use tokio::sync::Notify;

use crate::adb::Adb;
use crate::apps::Apps;
use crate::db::Db;
use crate::emulator::{Emulators, Machine};
use crate::iphone::Iphones;
use crate::journal::Journal;
use crate::known::Devices;
use crate::location::Locations;
use crate::rtc::{Hub, Peer};
use crate::settings::{self, Settings};
use crate::topics::Topics;
use crate::trust::Trust;
use crate::watch::Sighting;
use crate::{icon, media, vcam};

pub(crate) struct App {
    pub(crate) adb: Adb,
    pub(crate) iphones: Iphones,
    pub(crate) screens: crate::adb::Screens,
    pub(crate) scrcpy_server: std::path::PathBuf,
    pub(crate) android_helper: std::path::PathBuf,
    pub(crate) ffmpeg: std::path::PathBuf,
    pub(crate) emulators: Arc<Emulators>,
    pub(crate) apps: Apps,
    devices: RwLock<Vec<Device>>,
    pub(crate) known: Devices,
    pub(crate) locations: Locations,
    pub(crate) journal: Journal,
    pub(crate) trust: Trust,
    pub(crate) origins: Vec<String>,
    pub(crate) hub: Hub,
    pub(crate) remotes: media::Remotes,
    pub(crate) streams: media::Streams,
    pub(crate) nonces: Mutex<HashMap<Vec<u8>, Instant>>,
    pub(crate) topics: Arc<Topics>,
    pub(crate) browsers: Mutex<HashMap<Peer, Held>>,
    pub(crate) reported: Notify,
    pub(crate) looking: tokio::sync::Mutex<()>,
}

pub(crate) struct Held {
    pub(crate) client: ClientId,
    pub(crate) phones: Vec<UsbPhone>,
}

pub(crate) fn abis_of(device: &Device) -> Result<&[String]> {
    let described = device
        .described()
        .with_context(|| format!("install failed: {} not ready", mention(&device.id)))?;

    Ok(&described.abis)
}

impl App {
    pub(crate) async fn new(
        settings: Settings,
        data: &Path,
        setup: bool,
        rtc_port: u16,
    ) -> Result<Self> {
        let db = Arc::new(Db::open(&data.join("tetherd.db")).await?);
        let topics = Arc::new(Topics::default());
        let trust = Trust::new(db.clone(), data, setup, topics.clone()).await?;
        let machine = Machine {
            sdk: settings.android_sdk.clone(),
            satellite: settings.xwayland_satellite,
            data: data.to_path_buf(),
            cameras: vcam::Cameras::listen(settings.port)?,
            shim: settings::shim(settings.vcam),
        };

        Ok(Self {
            adb: Adb::new(&data.join("android"))?,
            iphones: Iphones::new(&data.join("ios"))?,
            screens: crate::adb::Screens::default(),
            scrcpy_server: settings.scrcpy_server,
            android_helper: settings.android_helper,
            ffmpeg: settings.ffmpeg.unwrap_or_else(|| "ffmpeg".into()),
            emulators: Arc::new(
                Emulators::new(machine, db.clone(), trust.primary(), topics.clone()).await?,
            ),
            apps: Apps::new(
                data.join("apps"),
                settings.android_sdk.as_deref().and_then(icon::tool),
                db.clone(),
                topics.clone(),
            )
            .await?,
            devices: RwLock::default(),
            locations: Locations::new(db.clone(), topics.clone()).await?,
            journal: Journal::new(db.clone(), topics.clone()).await?,
            trust,
            known: Devices::new(db).await?,
            origins: settings.origins,
            hub: Hub::start(rtc_port, settings::stun(settings.stun)),
            nonces: Mutex::default(),
            remotes: media::Remotes::default(),
            streams: media::Streams::default(),
            topics,
            browsers: Mutex::default(),
            reported: Notify::new(),
            looking: tokio::sync::Mutex::new(()),
        })
    }

    pub(crate) fn devices(&self) -> Vec<Device> {
        self.devices.read().unwrap().clone()
    }

    pub(crate) fn show(&self, devices: Vec<Device>) {
        let mut shown = self.devices.write().unwrap();
        let wants = |topic: &Topic| *topic == Topic::Devices;

        for device in &devices {
            if !shown.contains(device) {
                self.topics
                    .send(wants, &ToClient::Device(Box::new(device.clone())));
            }
        }

        for gone in shown.iter() {
            if devices.iter().all(|device| device.id != gone.id) {
                self.topics
                    .send(wants, &ToClient::DeviceGone(gone.id.clone()));
            }
        }

        *shown = devices;
    }

    pub(crate) fn device(&self, id: &DeviceId) -> Result<Device> {
        self.devices
            .read()
            .unwrap()
            .iter()
            .find(|device| device.id == *id)
            .cloned()
            .with_context(|| format!("device not found: {}", mention(id)))
    }

    pub(crate) fn is_ready(&self, id: &DeviceId) -> bool {
        self.device(id).is_ok_and(|device| device.ready())
    }

    /// Being listed makes a device's serial safe to pass as an argument.
    pub(crate) fn reached(&self, id: &DeviceId) -> Result<Device> {
        let device = self.device(id)?;
        let here = At::Instance(self.trust.primary());

        ensure!(
            device.ready() && device.connected().is_some_and(|way| way.at == here),
            "device not ready: {}",
            mention(id)
        );

        Ok(device)
    }

    /// tetherd's own emulators are stopped instead of disconnected.
    pub(crate) fn foreign(&self, id: &DeviceId) -> Result<Device> {
        let device = self.device(id)?;
        let ours = matches!(id, DeviceId::Emulator { port, .. } if self.emulators.owns(*port));

        ensure!(!ours, "{} is a managed emulator", mention(id));

        Ok(device)
    }

    pub(crate) fn browsers(&self) -> Vec<Sighting> {
        let browsers = self.browsers.lock().unwrap();

        browsers
            .values()
            .flat_map(|held| {
                held.phones.iter().map(|phone| Sighting {
                    connected: phone.held,
                    name: phone.name.clone(),
                    described: phone.described.clone(),
                    ..Sighting::route(
                        DeviceId::Android(phone.id.clone()),
                        Route {
                            way: Way {
                                means: Means::Usb,
                                at: At::Browser(held.client.clone()),
                            },
                            address: phone.id.clone(),
                            pairing: false,
                        },
                    )
                })
            })
            .collect()
    }

    pub(crate) fn holders(&self, serial: &str) -> Vec<(Peer, ClientId)> {
        let browsers = self.browsers.lock().unwrap();

        browsers
            .iter()
            .filter(|(_, held)| {
                held.phones
                    .iter()
                    .any(|phone| phone.id == serial && phone.held.is_some())
            })
            .map(|(peer, held)| (*peer, held.client.clone()))
            .collect()
    }
}
