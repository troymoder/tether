//! Each adb channel is a DataChannel of the page's WebRTC connection, labelled
//! with its device and service (`protocol::adb_label`). tetherd's first message
//! on it says how opening the adb channel went, and after that the bytes go
//! each way unchanged. A text message ends it, and the page then closes it
//! (`Hub::close`).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, ensure};
use protocol::{DeviceId, PART, encode};
use str0m::channel::ChannelId;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::{Semaphore, mpsc};

use crate::app::App;
use crate::rtc::{Credit, Hub, Peer};
use crate::task::Stopped;

/// The bytes from a device that may wait to go to the page on one channel.
/// A device that sends faster than the page takes waits for it.
const WINDOW: u32 = 1024 * 1024;

pub(crate) struct Channels {
    app: Arc<App>,
    hub: Hub,
    peer: Peer,
    open: HashMap<ChannelId, Relayed>,
}

struct Relayed {
    writes: mpsc::UnboundedSender<Vec<u8>>,
    _running: Stopped,
}

impl Channels {
    pub(crate) fn new(app: Arc<App>, hub: Hub, peer: Peer) -> Self {
        Self {
            app,
            hub,
            peer,
            open: HashMap::new(),
        }
    }

    pub(crate) fn open(&mut self, channel: ChannelId, device: &str, service: &str) {
        let (writes, written) = mpsc::unbounded_channel();
        let running = tokio::spawn(relay(
            self.app.clone(),
            (self.hub.clone(), self.peer, channel),
            (device.to_owned(), service.to_owned()),
            written,
        ));

        // Relays that ended, as when they failed to open, are dropped here.
        self.open.retain(|_, relayed| !relayed.writes.is_closed());
        self.open.insert(
            channel,
            Relayed {
                writes,
                _running: Stopped(running.abort_handle()),
            },
        );
    }

    /// True if this channel is an adb one, false if it belongs to the page.
    // shortcut: what the page writes waits here unbounded while the device
    // is slower. Hold back SCTP's window if large pushes ever pile up.
    pub(crate) fn write(&self, channel: ChannelId, data: Vec<u8>) -> bool {
        match self.open.get(&channel) {
            Some(relayed) => {
                let _ = relayed.writes.send(data);
                true
            }
            None => false,
        }
    }

    pub(crate) fn close(&mut self, channel: ChannelId) {
        self.open.remove(&channel);
    }
}

async fn relay(
    app: Arc<App>,
    (hub, peer, channel): (Hub, Peer, ChannelId),
    (device, service): (String, String),
    mut written: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let opened = open(&app, &device, &service).await;
    let result = opened
        .as_ref()
        .map(|_| ())
        .map_err(|error| format!("{error:#}"));

    hub.write(peer, channel, encode(&result), None);

    let Ok(stream) = opened else {
        hub.close(peer, channel);
        return;
    };

    let (mut reading, mut writing) = tokio::io::split(stream);
    let window = Arc::new(Semaphore::new(WINDOW as usize));
    // Dropping the stream closes the device's channel, so ending either
    // direction ends both.
    let from_device = async {
        let mut bytes = vec![0; PART];

        while let Ok(got @ 1..) = reading.read(&mut bytes).await {
            let Ok(permits) = window.acquire_many(got as u32).await else {
                return;
            };

            permits.forget();

            let credit = Credit(window.clone(), got as u32);

            hub.write(peer, channel, bytes[..got].to_vec(), Some(credit));
        }
    };

    let to_device = async {
        while let Some(data) = written.recv().await {
            if writing.write_all(&data).await.is_err() {
                return;
            }
        }
    };

    tokio::select! {
        () = from_device => {}
        () = to_device => {}
    }

    hub.close(peer, channel);
}

async fn open(app: &App, device: &str, service: &str) -> Result<DuplexStream> {
    // The page is trusted, but a NUL or a line break would end the service
    // early and make the device run something else.
    ensure!(
        !service.contains(['\0', '\n']),
        "adb failed: invalid service"
    );

    let id = device
        .parse::<DeviceId>()
        .map_err(|_| anyhow::anyhow!("adb failed: invalid device"))?;
    let device = app.reached(&id)?;
    let adb = app.adb.clone();
    let id = device.id.serial();
    let service = service.to_owned();

    // An open dropped half way puts the device's connection out of step
    // (libadb's "Dropping a call"). It runs on its own task so it finishes
    // when the relay is aborted, and the stream it gives then closes as it
    // is dropped.
    tokio::spawn(async move { adb.open(&id, &service).await }).await?
}
