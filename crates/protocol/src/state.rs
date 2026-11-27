use rkyv::{Archive, Deserialize, Serialize};

use crate::{CameraSettings, Lens, VirtualCamera};

/// The fingerprint of a tetherd's key: `SHA256:` and the base64 hash of its
/// public key.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(PartialEq, Eq, Hash))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct InstanceId(pub String);

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(PartialEq, Eq, Hash))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct ClientId(pub String);

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Webcam {
    /// Its device path, such as `/dev/video0`, which requests name it by.
    pub path: String,
    pub name: String,
    /// Largest first.
    pub sizes: Vec<WebcamSize>,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct WebcamSize {
    pub width: u16,
    pub height: u16,
    /// Highest first.
    pub rates: Vec<u16>,
    /// The pixel format, as ffmpeg names it: `mjpeg`, `yuyv422`, `yuv420p`
    /// or `nv12`.
    pub format: String,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Access {
    pub primary: InstanceId,
    pub trusted: Vec<Client>,
    pub asking: Vec<Client>,
    /// The primary first.
    pub instances: Vec<Instance>,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Instance {
    pub id: InstanceId,
    /// None until it is named, and the page shows the host it came from
    /// instead.
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Client {
    /// `SHA256:` and the base64 hash of its public key.
    pub print: String,
    pub name: String,
    pub kind: String,
    /// The time it last connected, or for a page that is asking, the time it
    /// asked, in milliseconds since the Unix epoch.
    pub at: u64,
}

/// `since` is in milliseconds since the Unix epoch.
#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Feeder {
    pub print: String,
    pub name: String,
    pub since: u64,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Place {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct EmulatorOptions {
    pub profiles: Vec<String>,
    /// System images, as sdkmanager names them.
    pub images: Vec<String>,
    /// True if tetherd has the shim, which gives emulators cameras.
    pub feeds_cameras: bool,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    pub condition: Condition,
    /// The ways it can be reached now, in the order Automatic tries them.
    pub routes: Vec<Route>,
    pub last: Option<Last>,
    pub emulator: Option<Run>,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Run {
    pub avd: String,
    pub status: Status,
    /// Sets the aspect ratio its camera sees
    /// ([`camera_shape`](crate::camera_shape)).
    pub upright: bool,
    pub camera: Option<Lens>,
    pub fed_by: Option<Feeder>,
    pub remote: Option<RemoteFeed>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(PartialEq, Eq, Hash))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum DeviceId {
    Iphone(String),
    /// By its serial number. A device found on the network is identified by
    /// the serial in the name it announces.
    Android(String),
    /// `port` is its console port.
    Emulator {
        port: u16,
        instance: InstanceId,
    },
}

impl DeviceId {
    pub fn platform(&self) -> Platform {
        match self {
            DeviceId::Iphone(_) => Platform::Ios,
            DeviceId::Android(_) | DeviceId::Emulator { .. } => Platform::Android,
        }
    }

    pub fn is_emulator(&self) -> bool {
        matches!(self, DeviceId::Emulator { .. })
    }

    /// The serial adb or the iPhone uses: the serial number, the UDID, or
    /// `emulator-<port>`.
    pub fn serial(&self) -> String {
        match self {
            DeviceId::Iphone(serial) | DeviceId::Android(serial) => serial.clone(),
            DeviceId::Emulator { port, .. } => format!("emulator-{port}"),
        }
    }
}

/// The id as one line, for the page's URL, events and messages:
/// `iphone:<udid>`, `android:<serial>` or `emulator:<port>@<instance>`. An
/// instance's fingerprint is base64 and can contain a `/`.
impl std::fmt::Display for DeviceId {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceId::Iphone(udid) => write!(out, "iphone:{udid}"),
            DeviceId::Android(serial) => write!(out, "android:{serial}"),
            DeviceId::Emulator { port, instance } => write!(out, "emulator:{port}@{}", instance.0),
        }
    }
}

impl std::str::FromStr for DeviceId {
    type Err = String;

    fn from_str(line: &str) -> Result<Self, String> {
        let unknown = || format!("unknown device id: {line}");
        let (kind, rest) = line.split_once(':').ok_or_else(unknown)?;

        match kind {
            "iphone" if !rest.is_empty() => Ok(DeviceId::Iphone(rest.to_owned())),
            "android" if !rest.is_empty() => Ok(DeviceId::Android(rest.to_owned())),
            "emulator" => {
                let (port, instance) = rest.split_once('@').ok_or_else(unknown)?;
                let port = port.parse().map_err(|_| unknown())?;

                Ok(DeviceId::Emulator {
                    port,
                    instance: InstanceId(instance.to_owned()),
                })
            }
            _ => Err(unknown()),
        }
    }
}

impl Device {
    pub fn ready(&self) -> bool {
        matches!(
            self.condition,
            Condition::Connected {
                state: DeviceState::Ready,
                ..
            }
        )
    }

    pub fn connected(&self) -> Option<&Way> {
        match &self.condition {
            Condition::Connected { way, .. } => Some(way),
            _ => None,
        }
    }

    pub fn described(&self) -> Option<&Described> {
        match &self.condition {
            Condition::Connected { described, .. } => described.as_ref(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Condition {
    /// `described` is None until it has reported what it runs.
    Connected {
        way: Way,
        state: DeviceState,
        described: Option<Described>,
    },
    Available,
    /// Held by something else, such as another browser or another adb.
    /// The holder cannot be identified.
    InUse,
    /// Connected before, and none of its routes is available now.
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Described {
    pub release: String,
    /// As Android names them, preferred first. Empty for an iPhone.
    pub abis: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Way {
    pub means: Means,
    pub at: At,
}

#[derive(Clone, Copy, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Means {
    Usb,
    Wifi,
    Emulator,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum At {
    Instance(InstanceId),
    Browser(ClientId),
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Route {
    pub way: Way,
    /// The adb address: `host:port` on the network, or the serial over USB.
    pub address: String,
    /// True while it shows a pairing code and must be paired before
    /// connecting.
    pub pairing: bool,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Last {
    pub way: Way,
    /// In milliseconds since the Unix epoch.
    pub at: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Platform {
    #[default]
    Android,
    Ios,
}

#[derive(Clone, Copy, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum DeviceState {
    Ready,
    Unauthorized,
    /// A phone being connected to, or an iPhone being paired or switched
    /// into QuickTime mode.
    Connecting,
    Offline,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct RemoteFeed {
    pub lens: Lens,
    pub source: RemoteSource,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum RemoteSource {
    Webcam {
        path: String,
        width: u16,
        height: u16,
        fps: u16,
    },
    /// A looping video or a still picture, by its absolute path or a path
    /// in the home folder (`~/…`).
    File { path: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Status {
    Shutdown,
    Booting,
    Booted,
    Stopping,
}

// tetherd stores these as JSON in its database (crates/server/src/db/emulators.rs),
// so serde is derived unconditionally.
#[derive(
    Clone, Debug, PartialEq, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct EmulatorSettings {
    /// One of [`EmulatorOptions::profiles`].
    pub profile: String,
    /// One of [`EmulatorOptions::images`].
    pub image: String,
    /// RAM in megabytes.
    pub memory: u32,
    pub graphics: Graphics,
    pub autostart: bool,
    /// The emulator takes only one camera fed through the shim, and with two
    /// it sees only the back one.
    pub camera: Option<VirtualCamera>,
}

impl EmulatorSettings {
    pub fn camera(&self, lens: Lens) -> Option<CameraSettings> {
        self.camera
            .filter(|camera| camera.lens == lens)
            .map(|camera| camera.settings)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Graphics {
    /// The host's GPU where tetherd finds a desktop to draw through, and
    /// software rendering where it does not.
    Gpu,
    Software,
}
