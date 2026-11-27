use std::any::Any;

use android::scrcpy;
use async_trait::async_trait;
use futures_channel::mpsc;
use futures_util::future::LocalBoxFuture;
use protocol::{DeviceId, EmulatorSettings, Feeder, InstanceId, PhoneCamera, RemoteFeed, Status};
use web_sys::MediaStream;

use crate::device::unsupported;
use crate::link::Link;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Emulator {
    /// Its virtual device's name, which it is asked for by.
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) instance: InstanceId,
    pub(crate) settings: EmulatorSettings,
    pub(crate) status: Status,
    pub(crate) upright: bool,
    pub(crate) fed_by: Option<Feeder>,
    pub(crate) remote: Option<RemoteFeed>,
}

impl Emulator {
    /// None until the emulator's settings have come (`Topic::Emulator`).
    pub(crate) fn of(link: Link, id: &DeviceId) -> Option<Self> {
        let DeviceId::Emulator { instance, .. } = id else {
            return None;
        };

        let device = link.device(id)?;
        let run = device.emulator?;
        let settings = link.settings(&run.avd)?;

        Some(Emulator {
            name: run.avd,
            label: device.name,
            instance: instance.clone(),
            settings,
            status: run.status,
            upright: run.upright,
            fed_by: run.fed_by,
            remote: run.remote,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Kind {
    None,
    Fed(Emulator),
    Mirrored,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Control {
    Torch(bool),
    /// A factor, 1 for none.
    Zoom(f32),
}

/// Dropping it lets the camera go.
pub(crate) struct Mirrored {
    pub(crate) stream: MediaStream,
    pub(crate) control: mpsc::UnboundedSender<Control>,
    /// Resolves to the reason it stopped.
    pub(crate) ended: LocalBoxFuture<'static, String>,
    pub(crate) kept: Box<dyn Any>,
}

#[async_trait(?Send)]
pub(crate) trait Camera {
    /// Tracked, as an emulator's settings and feed change.
    fn kind(&self) -> Kind {
        Kind::None
    }

    /// Its own cameras. Only Android phones list cameras, so this is
    /// scrcpy's type.
    async fn cameras(&self) -> Result<Vec<scrcpy::Camera>, String> {
        Err(unsupported("camera"))
    }

    async fn mirror(&self, _camera: PhoneCamera) -> Result<Mirrored, String> {
        Err(unsupported("camera"))
    }
}
