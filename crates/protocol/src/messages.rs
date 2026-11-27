use rkyv::{Archive, Deserialize, Serialize};

use crate::{
    Access, App, Described, Device, DeviceId, DeviceState, EmulatorSettings, EventPage, EventQuery,
    Feeder, Framing, InstanceId, Lens, Place, RemoteFeed, Way,
};

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum ToServer {
    /// [`ToClient::Reply`] with the same id gives the outcome.
    Call {
        id: u32,
        request: Request,
    },
    /// The page sends this and then [`ToServer::Query`] for the same topic.
    /// The answer includes every change sent before it, and every change
    /// replaces a whole item, so applying them in arrival order misses none.
    /// A page has one events query at a time, and subscribing again replaces
    /// it.
    Subscribe(Topic),
    /// For events, this applies whatever the query.
    Unsubscribe(Topic),
    Query(Topic),
    /// An action the page did itself, recorded under the page's name.
    Event {
        what: String,
        failed: bool,
        message: String,
        about: Option<DeviceId>,
    },
    /// Sent when the page adds or drops tracks to watch a screen.
    /// [`ToClient::Reply`] with the same id gives tetherd's answer.
    Offer {
        id: u32,
        sdp: String,
    },
    /// Every phone on the page's WebUSB, sent again whenever that changes.
    /// Forgotten when the page disconnects.
    Usb(Vec<UsbPhone>),
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum ToClient {
    /// A line to show the user, or the error.
    Reply {
        id: u32,
        result: Result<String, String>,
    },
    /// The answer to a [`ToServer::Query`], and each change of a topic that
    /// is always sent whole.
    Value(Value),
    Device(Box<Device>),
    DeviceGone(DeviceId),
    App(App),
    AppGone(String),
    /// Asks the page to release a phone it holds over WebUSB, because another
    /// connection is taking it.
    LetGo {
        device: DeviceId,
    },
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Topic {
    /// Each device is listed once, however many ways lead to it.
    Devices,
    Access,
    Apps,
    Places,
    /// Each new event resends the whole page of results.
    Events(EventQuery),
    /// By its AVD name.
    Emulator(String),
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Value {
    Devices(Vec<Device>),
    Access(Access),
    Apps(Vec<App>),
    Places(Vec<Place>),
    Events(EventPage),
    /// None when no emulator has that name.
    Emulator {
        name: String,
        settings: Option<EmulatorSettings>,
    },
}

/// A phone the page holds, or one it was granted before and could claim.
#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct UsbPhone {
    /// The serial number.
    pub id: String,
    pub name: String,
    /// None while the page could claim it and does not hold it.
    pub held: Option<DeviceState>,
    /// None until it has reported what it runs.
    pub described: Option<Described>,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Request {
    Approve {
        print: String,
    },
    Reject {
        print: String,
    },
    Revoke {
        print: String,
    },
    Rename {
        print: String,
        name: String,
    },
    RenameInstance {
        name: String,
    },
    RenameDevice {
        device: DeviceId,
        name: String,
    },
    /// The phone reappears once it connects again.
    ForgetDevice {
        device: DeviceId,
    },
    CreateEmulator {
        name: String,
        settings: EmulatorSettings,
    },
    /// The settings take effect the next time the emulator starts.
    Configure {
        name: String,
        settings: EmulatorSettings,
    },
    Delete {
        name: String,
    },
    Erase {
        name: String,
    },
    /// Answered once it is starting, before it has booted.
    Start {
        name: String,
    },
    /// Answered once it has exited.
    Stop {
        name: String,
    },
    /// Each line of the reply is the time it was printed in milliseconds
    /// since the Unix epoch, a tab, and the text.
    EmulatorLog {
        name: String,
    },
    /// Connects through `way`, or else the first of the device's routes. A
    /// device is connected in one place at a time, so one connected elsewhere
    /// is refused unless `take` drops that connection first. `code` pairs a
    /// route that shows a pairing code. The page sends this for its own
    /// WebUSB route before it uses a phone it holds. tetherd never reconnects
    /// a device by itself.
    Connect {
        device: DeviceId,
        way: Option<Way>,
        code: Option<String>,
        take: bool,
    },
    /// Pairs when there is a code, and connects otherwise.
    Reach {
        /// `host:port`.
        address: String,
        code: Option<String>,
    },
    /// The device stays disconnected until a connect request.
    Disconnect {
        device: DeviceId,
    },
    /// `id` is the package name. `from` is the device the package was pulled
    /// from, if any, which the event is filed under.
    HandOver {
        id: String,
        apk: Vec<u8>,
        from: Option<DeviceId>,
    },
    /// Replaces any installed version, newer or older.
    InstallApp {
        device: DeviceId,
        id: String,
    },
    /// Installed copies stay installed.
    ForgetApp {
        id: String,
    },
    AutoInstall {
        id: String,
        device: DeviceId,
        on: bool,
    },
    /// For Android before 12, which has no test provider commands over adb.
    GeoFix {
        device: DeviceId,
        latitude: f64,
        longitude: f64,
    },
    /// Replaces a place of the same name.
    KeepPlace(Place),
    ForgetPlace {
        name: String,
    },
    /// The reply is the package name inside the APK. With a device, the APK
    /// must be able to run there.
    AddApp {
        apk: Vec<u8>,
        device: Option<DeviceId>,
    },
    /// The reply is a self-contained SVG.
    AppIcon {
        id: String,
    },
    /// The requesting page becomes the camera's feeder.
    TakeCamera {
        emulator: String,
    },
    /// Stops a [`Media::Remote`] feed, which leaves the camera black.
    StopRemote {
        emulator: String,
    },
}

/// Sent as the page's first message on a media channel. Every message on the
/// channel, this one included, is split into [`parts`](crate::parts) and
/// reassembled with a [`Gather`](crate::Gather) of its own. Closing the
/// channel ends the job. tetherd signals its close with a text message,
/// because every part is binary, and the page then closes the channel.
#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Media {
    /// A call too large for the control channel, such as one carrying an
    /// APK. tetherd answers with `Result<String, String>` and closes.
    Call { request: Request },
    /// Answered with `Result<Vec<u8>, String>` before closing.
    ScrcpyServer,
    /// Answered with `Result<Vec<u8>, String>` before closing.
    AndroidHelper,
    /// Pictures of this emulator's screen while it boots or stops, each a
    /// [`Picture`]. Once Android is up the page shows the screen itself
    /// through scrcpy.
    Watch { device: DeviceId },
    /// tetherd performs the operation with the phone's pair record, so the
    /// key never leaves tetherd. It answers `Result<Vec<u8>, String>`. For
    /// `Open` the result is empty, and the channel then carries the service's
    /// plain bytes both ways. For the others the channel closes.
    Ios {
        device: DeviceId,
        operation: IosOperation,
    },
    /// Streams a device tetherd holds as H.264 and Opus to the page's WebRTC
    /// tracks with these mids, which the page added with a
    /// [`ToServer::Offer`]. tetherd captures the device once however many
    /// pages watch it. An iPhone's audio does not play on the phone
    /// meanwhile. An Android device takes the page's input on the channel.
    /// If capture fails, tetherd sends a [`Screen`] and closes the channel.
    Screen {
        device: DeviceId,
        video: String,
        audio: String,
        camera: Option<PhoneCamera>,
    },
    /// Frames for this emulator's camera, each a [`CameraFrame`]. Opening
    /// this makes the page the feeder, replacing tetherd's own feed. While
    /// another page feeds the camera, or when one takes it over, tetherd
    /// sends a [`CameraHeld`] and closes the channel. Closing the channel
    /// leaves the camera black until something else feeds it.
    Camera {
        emulator: String,
        lens: Lens,
        lossless: bool,
    },
    /// Feeds this emulator's camera from a source on tetherd's machine,
    /// unless a page feeds it. The feed belongs to no page and runs until it
    /// is stopped ([`Request::StopRemote`]), a page feeds the camera, or the
    /// emulator stops. After starting, the channel behaves like
    /// [`Media::Preview`].
    Remote { emulator: String, feed: RemoteFeed },
    /// tetherd first sends `Result<Framing, String>` with the current
    /// framing, then [`Previewed`] messages until the feed stops. A
    /// [`Framing`] any page sends on the channel is applied by tetherd and
    /// forwarded to the other pages.
    Preview { emulator: String },
    /// Answered with the APK as `Result<Vec<u8>, String>` before closing.
    Build { app: String, build: u32 },
    /// Answered with `Result<Looked, String>` before closing. The path is
    /// absolute, or in the home folder (`~/…`).
    Look { path: String },
    /// Answered with `Result<Vec<Match>, String>` before closing.
    Find { path: String, sought: String },
    /// Answered with `Result<Vec<Webcam>, String>` before closing.
    Webcams { instance: InstanceId },
    /// Pairs with the Android phone that scans a QR code naming `name`, and
    /// connects to it. Each step arrives as a [`Pairing`]. Closing the
    /// channel stops waiting.
    Pair { name: String, password: String },
    /// Answered with `Result<EmulatorOptions, String>` before closing. An
    /// error when the instance has no Android SDK.
    EmulatorOptions { instance: InstanceId },
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Looked {
    /// `path` is absolute. `items` are the entries that can be shown:
    /// folders first, then videos and pictures.
    Folder { path: String, items: Vec<Item> },
    /// `path` is absolute.
    File { path: String },
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Item {
    pub name: String,
    pub kind: ItemKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum ItemKind {
    Folder { items: u32 },
    File { size: u64 },
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Match {
    /// Relative to the folder searched.
    pub path: String,
    pub folder: bool,
}

/// The services' own protocols are handled by the requesting side, as with
/// adb channels.
#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum IosOperation {
    /// Returns the service's stream with any TLS the phone requires already
    /// set up.
    Open {
        service: String,
    },
    /// A binary plist. Without a key, the whole domain.
    Value {
        domain: Option<String>,
        key: Option<String>,
    },
    /// The value is a binary plist.
    SetValue {
        domain: Option<String>,
        key: String,
        value: Vec<u8>,
    },
    RemoveValue {
        domain: Option<String>,
        key: String,
    },
}

/// Needs Android 12 or later, and has no audio. Nothing else on the device
/// can use the camera meanwhile.
#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct PhoneCamera {
    /// Its id in scrcpy's camera list.
    pub id: String,
    /// None lets scrcpy choose.
    pub mode: Option<CameraMode>,
    /// 1 for none.
    pub zoom: f32,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct CameraMode {
    /// Such as `1920x1080`.
    pub size: String,
    pub rate: u32,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Pairing {
    Scanned {
        address: String,
    },
    Connecting,
    /// The last step.
    Connected {
        device: DeviceId,
    },
    /// Paired, but the phone offered no connection in time. The last step.
    Paired,
    /// As `pair failed: …`. The last step.
    Failed(String),
}

/// A message a [`Media::Screen`] channel carries to the page, while the media
/// goes to the tracks. The page sends an Android device's input on the same
/// channel, as scrcpy control messages.
#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Screen {
    /// As WebCodecs names it, such as `avc1.640033`. It comes first, and
    /// again when it changes, as when an Android device's screen rotates.
    Codec(String),
    /// An Android device's clipboard text, each time it changes.
    Clipboard(String),
    /// As `screen failed: …`.
    Failed(String),
}

/// RGBA, packed with [`squeeze`](crate::squeeze).
#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Picture {
    pub width: u16,
    pub height: u16,
    pub data: Vec<u8>,
}

/// A lossless frame is planar YUV 4:2:0 in limited range, packed with
/// [`pack`](crate::pack). Otherwise it is H.264 as the browser encoded it
/// (Annex B), each frame following the previous one, starting with a key
/// frame.
#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct CameraFrame {
    pub width: u16,
    pub height: u16,
    pub data: Vec<u8>,
}

/// Another page feeds the camera, either since before the channel opened or
/// because it took the camera over (`taken`).
#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct CameraHeld {
    pub by: Feeder,
    pub taken: bool,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Previewed {
    /// The camera feed as a downscaled JPEG, sent for only some frames.
    Picture(Vec<u8>),
    /// The framing another page chose.
    Framing(Framing),
}
