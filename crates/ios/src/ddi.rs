//! The phone's developer services, such as the HID service input goes
//! through, come from the developer disk image. On iOS 17 and later it is a
//! cryptex, installed through `cryptexd` in the CoreDeviceProxy tunnel once
//! Apple's signing server (TSS) personalized it for the phone. It stays
//! installed until the phone restarts.
//!
//! The image is a folder as Xcode's personalized images are laid out:
//! `BuildManifest.plist`, `Image.dmg` and its `.trustcache`,
//! `.root_hash` and `.cryptex_info`.

use std::path::Path;

use anyhow::{Result, anyhow, bail};
use idevice::Idevice;
use idevice::core_device_proxy::CoreDeviceProxy;
use idevice::cryptexd::{Cryptex1Assets, install_ddi, installed_ddi};
use idevice::rsd::RsdHandshake;
use protocol::IosOperation;

use crate::input::TUNNEL;
use crate::perform::{Done, perform};
use crate::phone::{LABEL, Phone, Ports};

#[derive(Debug, PartialEq, Eq)]
pub enum Mounted {
    Already,
    Now,
}

fn failed(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("ddi failed: {error}")
}

pub async fn mount<P: Ports>(phone: &Phone<P>, folder: &Path) -> Result<Mounted> {
    if !developer_mode(phone).await? {
        bail!("ddi failed: Developer Mode is off");
    }

    let open = IosOperation::Open {
        service: TUNNEL.to_owned(),
    };
    let Done::Stream(tunnel) = perform(phone, &open).await? else {
        bail!("ddi failed: no tunnel");
    };
    let proxy = CoreDeviceProxy::new(Idevice::new(tunnel, LABEL))
        .await
        .map_err(failed)?;
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let mut adapter = proxy
        .create_software_tunnel()
        .map_err(failed)?
        .to_async_handle();
    let rsd = adapter.connect(rsd_port).await.map_err(failed)?;
    let mut handshake = RsdHandshake::new(rsd).await.map_err(failed)?;

    if installed_ddi(&mut adapter, &mut handshake)
        .await
        .map_err(failed)?
        .is_some()
    {
        return Ok(Mounted::Already);
    }

    let assets = Cryptex1Assets::load(folder)
        .await
        .map_err(|error| anyhow!("ddi failed: {}: {error}", folder.display()))?;

    install_ddi(&mut adapter, &mut handshake, &assets)
        .await
        .map_err(failed)?;

    Ok(Mounted::Now)
}

/// Lockdown's AMFI domain has this key on iOS 16 and later.
async fn developer_mode<P: Ports>(phone: &Phone<P>) -> Result<bool> {
    let ask = IosOperation::Value {
        domain: Some("com.apple.security.mac.amfi".to_owned()),
        key: Some("DeveloperModeStatus".to_owned()),
    };
    let Done::Value(value) = perform(phone, &ask).await? else {
        bail!("ddi failed: no Developer Mode status");
    };

    plist::from_bytes(&value).map_err(failed)
}
