mod avd;
mod camera;
mod launch;
mod log;
mod sdk;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use protocol::{EmulatorOptions, EmulatorSettings, Feeder, InstanceId, Run, Status, Topic, Value};
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, OnceCell, watch};

use crate::boot;
use crate::camera::Fed;
use crate::db::Db;
use crate::topics::Topics;
use crate::vcam::Cameras;

pub(crate) struct Emulators {
    sdk: Option<PathBuf>,
    satellite: Option<PathBuf>,
    data: PathBuf,
    db: Arc<Db>,
    list: Mutex<Vec<Entry>>,
    /// Making, starting and removing happen one at a time.
    busy: tokio::sync::Mutex<()>,
    /// The SDK's profiles and images, read once because they do not change
    /// while this runs.
    options: OnceCell<Option<EmulatorOptions>>,
    cameras: Arc<Cameras>,
    shim: Option<PathBuf>,
    instance: InstanceId,
    topics: Arc<Topics>,
}

pub(crate) struct Machine {
    pub(crate) sdk: Option<PathBuf>,
    /// The X server an emulator reaches the GPU through on Linux.
    pub(crate) satellite: Option<PathBuf>,
    pub(crate) data: PathBuf,
    pub(crate) cameras: Arc<Cameras>,
    /// `libvcam.so`. Without it emulators have no cameras.
    pub(crate) shim: Option<PathBuf>,
}

struct Entry {
    saved: Saved,
    running: Option<Running>,
    /// Its virtual device, display and cameras are made before it runs, which
    /// takes a while.
    starting: bool,
    /// Kept after the emulator stops so the page can show why.
    log: Arc<Mutex<String>>,
    /// Frames from any other client are dropped.
    feeder: Option<Feeder>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub name: String,
    pub port: u16,
    pub settings: EmulatorSettings,
}

#[derive(Clone)]
struct Running {
    exited: watch::Receiver<bool>,
    kill: Arc<Notify>,
    stopping: Arc<AtomicBool>,
    /// True once Android has been up in this run. One that goes down after
    /// that without being told to was shut down from Android's own menu.
    booted: Arc<AtomicBool>,
    /// The emulator names its own files after its process.
    pid: Option<u32>,
    /// Back, then front.
    cameras: [Option<Arc<Fed>>; 2],
}

impl Running {
    /// True while Android goes down, told to from here or shut down from its
    /// own menu. That menu drops adb at once while the emulator runs on until
    /// Android is down. One restarted from that menu is up again once adb is.
    fn down(&self, connected: bool) -> bool {
        self.stopping.load(Ordering::Relaxed) || (self.booted.load(Ordering::Relaxed) && !connected)
    }
}

impl Emulators {
    pub(crate) async fn new(
        machine: Machine,
        db: Arc<Db>,
        instance: InstanceId,
        topics: Arc<Topics>,
    ) -> Result<Self> {
        let saved = db.emulators().await?;

        Ok(Self {
            sdk: machine.sdk,
            satellite: machine.satellite,
            data: machine.data,
            db,
            list: Mutex::new(saved.into_iter().map(Entry::stopped).collect()),
            busy: Default::default(),
            options: OnceCell::new(),
            cameras: machine.cameras,
            shim: machine.shim,
            instance,
            topics,
        })
    }

    pub(crate) fn settings(&self, name: &str) -> Option<EmulatorSettings> {
        let list = self.list.lock().unwrap();

        list.iter()
            .find(|entry| entry.saved.name == name)
            .map(|entry| entry.saved.settings.clone())
    }

    fn changed(&self, name: &str) {
        self.topics.value(
            &Topic::Emulator(name.to_owned()),
            Value::Emulator {
                name: name.to_owned(),
                settings: self.settings(name),
            },
        );
    }

    pub(crate) fn owns(&self, port: u16) -> bool {
        let list = self.list.lock().unwrap();

        list.iter().any(|entry| entry.saved.port == port)
    }

    pub(crate) fn avd_on(&self, port: u16) -> Option<String> {
        let list = self.list.lock().unwrap();

        list.iter()
            .find(|entry| entry.saved.port == port)
            .map(|entry| entry.saved.name.clone())
    }

    pub(crate) fn port_of(&self, name: &str) -> Option<u16> {
        let list = self.list.lock().unwrap();

        list.iter()
            .find(|entry| entry.saved.name == name)
            .map(|entry| entry.saved.port)
    }

    pub(crate) fn running(&self) -> Vec<u16> {
        let list = self.list.lock().unwrap();

        list.iter()
            .filter(|entry| entry.running.is_some())
            .map(|entry| entry.saved.port)
            .collect()
    }

    /// Telling an emulator booted is what spots it being shut down from
    /// Android's own menu later.
    pub(crate) fn list(
        &self,
        ready: impl Fn(&str) -> bool,
        connected: impl Fn(&str) -> bool,
    ) -> Vec<(u16, Run)> {
        let list = self.list.lock().unwrap();

        list.iter()
            .map(|entry| {
                let address = format!("emulator-{}", entry.saved.port);
                // Every fed camera is told the emulator's turn, so any one of
                // them knows it.
                let upright = entry
                    .running
                    .iter()
                    .flat_map(|running| running.cameras.iter().flatten())
                    .next()
                    .is_none_or(|camera| camera.upright());
                let run = Run {
                    avd: entry.saved.name.clone(),
                    status: match &entry.running {
                        None if entry.starting => Status::Booting,
                        None => Status::Shutdown,
                        Some(running) if running.down(connected(&address)) => Status::Stopping,
                        Some(running) if ready(&address) => {
                            running.booted.store(true, Ordering::Relaxed);
                            Status::Booted
                        }
                        Some(_) => Status::Booting,
                    },
                    upright,
                    camera: entry.saved.settings.camera.map(|camera| camera.lens),
                    fed_by: entry.running.as_ref().and(entry.feeder.clone()),
                    // Filled in from `media::Remotes` by whoever lists it.
                    remote: None,
                };

                (entry.saved.port, run)
            })
            .collect()
    }

    pub(crate) fn runs(&self, name: &str) -> bool {
        self.list
            .lock()
            .unwrap()
            .iter()
            .any(|entry| entry.saved.name == name && entry.running.is_some())
    }

    /// The port of its gRPC service, its process, and true once it has been
    /// told to stop.
    pub(crate) fn pictured(&self, port: u16, connected: bool) -> Option<(u16, u32, bool)> {
        let list = self.list.lock().unwrap();
        let entry = list.iter().find(|entry| entry.saved.port == port)?;
        let running = entry.running.as_ref()?;
        let stopping = running.down(connected);

        Some((boot::port(entry.saved.port), running.pid?, stopping))
    }

    fn find(&self, name: &str) -> Result<(Saved, Option<Running>)> {
        let list = self.list.lock().unwrap();
        let entry = list
            .iter()
            .find(|entry| entry.saved.name == name)
            .with_context(|| format!("emulator not found: {name}"))?;

        Ok((entry.saved.clone(), entry.running.clone()))
    }

    async fn save(&self) -> Result<()> {
        let _writing = self.db.writing().await;
        let saved: Vec<Saved> = {
            let list = self.list.lock().unwrap();

            list.iter().map(|entry| entry.saved.clone()).collect()
        };

        self.db.save_emulators(&saved, &self.instance).await
    }

    fn set_starting(&self, name: &str, starting: bool) {
        let mut list = self.list.lock().unwrap();

        if let Some(entry) = list.iter_mut().find(|entry| entry.saved.name == name) {
            entry.starting = starting;
        }
    }

    fn set_running(&self, name: &str, running: Option<Running>) {
        let mut list = self.list.lock().unwrap();

        if let Some(entry) = list.iter_mut().find(|entry| entry.saved.name == name) {
            // A new run makes its cameras again, fed by whoever starts to.
            entry.feeder = None;
            entry.running = running;
        }
    }
}

impl Entry {
    fn stopped(saved: Saved) -> Self {
        Self {
            saved,
            running: None,
            starting: false,
            log: Default::default(),
            feeder: None,
        }
    }
}
