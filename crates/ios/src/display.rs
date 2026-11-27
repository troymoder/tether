//! `com.apple.iokit.hid.displayStatus` is the first Darwin notification of
//! each screen change (iOS 27.3, ios-qvh's research/runs/screen-probe.log).
//! It carries no value, so the watcher has to tell which way the screen
//! went, as by whether frames were coming just before.

use anyhow::{Result, anyhow};
use idevice::IdeviceService;
use idevice::notification_proxy::NotificationProxyClient;

use crate::phone::{Phone, Ports};

const DISPLAY_STATUS: &str = "com.apple.iokit.hid.displayStatus";

pub struct Display {
    proxy: NotificationProxyClient,
}

fn failed(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("display failed: {error}")
}

impl Display {
    pub async fn watch<P: Ports>(phone: &Phone<P>) -> Result<Self> {
        let mut proxy = NotificationProxyClient::connect(phone)
            .await
            .map_err(failed)?;

        proxy
            .observe_notification(DISPLAY_STATUS)
            .await
            .map_err(failed)?;

        Ok(Self { proxy })
    }

    pub async fn changed(&mut self) -> Result<()> {
        loop {
            let name = self.proxy.receive_notification().await.map_err(failed)?;

            if name == DISPLAY_STATUS {
                return Ok(());
            }
        }
    }
}
