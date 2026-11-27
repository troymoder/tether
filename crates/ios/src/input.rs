//! Input goes through CoreDevice's Indigo HID service, as Xcode's device
//! remote control sends it. The service needs Developer Mode and the
//! developer disk image.
//!
//! idevice's and go-ios's tools describe an "auth gate": the phone takes input
//! only while CoreDevice's display stream runs. tether does not start that
//! stream, because it blanks the phone's camera.

use anyhow::{Result, anyhow};
use idevice::core_device::{
    ButtonState, DigitizerEdge, DigitizerEventType, DigitizerTarget, IndigoHidClient,
};
use idevice::core_device_proxy::CoreDeviceProxy;
use idevice::rsd::RsdHandshake;
pub use idevice::springboardservices::InterfaceOrientation;
use idevice::springboardservices::SpringBoardServicesClient;
use idevice::tcp::handle::AdapterHandle;
use idevice::{Idevice, ReadWrite, RsdService};

use crate::phone::LABEL;

mod keys;

pub const TUNNEL: &str = "com.apple.internal.devicecompute.CoreDeviceProxy";

/// A fraction of the screen. iOS only recognizes going home (up from the
/// bottom), Notification and Control Center (down from the top) and going
/// back (in from the left) from a touch that starts this close to the edge
/// and is sent as coming in over it.
const EDGE: f64 = 0.03;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Down,
    Move,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Button {
    Home,
    Power,
    VolumeUp,
    VolumeDown,
}

impl Button {
    /// The HID usage page and usage, as idevice's `hid` tool sends them.
    fn usage(self) -> (u64, u64) {
        match self {
            Button::Home => (0x0c, 0x40),
            Button::Power => (0x0c, 0x30),
            Button::VolumeUp => (0x0c, 0xe9),
            Button::VolumeDown => (0x0c, 0xea),
        }
    }
}

fn edge_at(x: f64, y: f64) -> DigitizerEdge {
    if y >= 1.0 - EDGE {
        DigitizerEdge::Bottom
    } else if y <= EDGE {
        DigitizerEdge::Top
    } else if x <= EDGE {
        DigitizerEdge::Left
    } else if x >= 1.0 - EDGE {
        DigitizerEdge::Right
    } else {
        DigitizerEdge::None
    }
}

pub struct Input {
    indigo: IndigoHidClient<Box<dyn ReadWrite>>,
    /// Holds the tunnel open.
    _adapter: AdapterHandle,
    /// A touch's moves and its end carry the edge it came in over.
    edge: DigitizerEdge,
}

fn failed(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("input failed: {error}")
}

impl Input {
    pub async fn open(tunnel: Box<dyn ReadWrite>) -> Result<Self> {
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
        let indigo = IndigoHidClient::connect_rsd(&mut adapter, &mut handshake)
            .await
            .map_err(failed)?;

        Ok(Self {
            indigo,
            _adapter: adapter,
            edge: DigitizerEdge::None,
        })
    }

    /// `x` and `y` each run 0 to 1 across the phone's screen as it is upright.
    pub async fn touch(&mut self, phase: Phase, x: f64, y: f64) -> Result<()> {
        let x = x.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);
        let kind = match phase {
            Phase::Down => {
                self.edge = edge_at(x, y);

                DigitizerEventType::Start
            }
            Phase::Move => DigitizerEventType::Position,
            Phase::Up => DigitizerEventType::End,
        };

        self.indigo
            .send_digitizer((x, y), None, kind, self.edge, DigitizerTarget::MainScreen)
            .await
            .map_err(failed)
    }

    pub async fn button(&mut self, button: Button, down: bool) -> Result<()> {
        let (page, usage) = button.usage();
        let state = if down {
            ButtonState::Down
        } else {
            ButtonState::Up
        };

        self.indigo
            .send_button(page, usage, state)
            .await
            .map_err(failed)
    }

    pub async fn key(&mut self, code: &str, down: bool) -> Result<()> {
        let Some(usage) = keys::usage(code) else {
            return Ok(());
        };
        let state = if down {
            ButtonState::Down
        } else {
            ButtonState::Up
        };

        self.indigo
            .send_keyboard(u64::from(usage), state)
            .await
            .map_err(failed)
    }
}

pub struct Orientation {
    springboard: SpringBoardServicesClient,
}

impl Orientation {
    /// [`Orientation::open`] takes this service's stream with its TLS done.
    pub const SERVICE: &str = "com.apple.springboardservices";

    pub fn open(stream: Box<dyn ReadWrite>) -> Self {
        Self {
            springboard: SpringBoardServicesClient::new(Idevice::new(stream, LABEL)),
        }
    }

    pub async fn get(&mut self) -> Result<InterfaceOrientation> {
        self.springboard
            .get_interface_orientation()
            .await
            .map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_touch_at_an_edge_comes_in_over_it() {
        assert_eq!(edge_at(0.5, 0.99), DigitizerEdge::Bottom);
        assert_eq!(edge_at(0.5, 0.01), DigitizerEdge::Top);
        assert_eq!(edge_at(0.01, 0.5), DigitizerEdge::Left);
        assert_eq!(edge_at(0.99, 0.5), DigitizerEdge::Right);
        assert_eq!(edge_at(0.5, 0.5), DigitizerEdge::None);
    }
}
