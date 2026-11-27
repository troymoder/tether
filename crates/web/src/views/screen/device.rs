use std::any::Any;

use async_trait::async_trait;
use futures_channel::mpsc;
use web_sys::MediaStream;

use crate::device::unsupported;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Button {
    Back,
    Home,
    Recents,
    Power,
    VolumeUp,
    VolumeDown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TouchAction {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Input {
    /// `x` and `y` are in pixels of the shown video frame, which was `width`
    /// by `height`.
    Touch {
        action: TouchAction,
        pointer: u32,
        x: f32,
        y: f32,
        width: u16,
        height: u16,
    },
    Key {
        down: bool,
        /// `KeyboardEvent.code`.
        code: String,
        shift: bool,
        ctrl: bool,
        alt: bool,
    },
    Text(String),
    /// `dx` and `dy` are in notches of a mouse wheel, positive to the right
    /// and up.
    Scroll {
        x: f32,
        y: f32,
        width: u16,
        height: u16,
        dx: f32,
        dy: f32,
    },
    /// Pressed and released at once.
    Button(Button),
    /// `repeat` counts from 1 while the button stays down, and is 0 otherwise.
    Hold {
        button: Button,
        down: bool,
        repeat: u32,
    },
    /// The text arrives as [`Told::Clipboard`].
    ReadClipboard,
    Clipboard(String),
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Features: u16 {
        const STREAMS = 1 << 0;
        const ROTATES = 1 << 1;
        const BACK = 1 << 2;
        const HOME = 1 << 3;
        const RECENTS = 1 << 4;
        const POWER = 1 << 5;
        const VOLUME_UP = 1 << 6;
        const VOLUME_DOWN = 1 << 7;
    }
}

impl Features {
    pub(crate) const PHONE_BUTTONS: Features = Features::HOME
        .union(Features::POWER)
        .union(Features::VOLUME_UP)
        .union(Features::VOLUME_DOWN);

    pub(crate) fn button(button: Button) -> Features {
        match button {
            Button::Back => Features::BACK,
            Button::Home => Features::HOME,
            Button::Recents => Features::RECENTS,
            Button::Power => Features::POWER,
            Button::VolumeUp => Features::VOLUME_UP,
            Button::VolumeDown => Features::VOLUME_DOWN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Turned {
    /// Degrees counter-clockwise from upright.
    pub(crate) degrees: u16,
    /// True while it rotates by its accelerometer, where that is known.
    pub(crate) auto: Option<bool>,
}

pub(crate) struct Session {
    pub(crate) stream: MediaStream,
    pub(crate) input: mpsc::UnboundedSender<Input>,
    /// The last one is [`Told::Ended`].
    pub(crate) told: mpsc::UnboundedReceiver<Told>,
    /// Keeps the screen running, and ends it when dropped.
    pub(crate) kept: Box<dyn Any>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Told {
    /// The video's codec as WebCodecs names it. It is sent again when the
    /// video changes shape, such as when the screen rotates.
    Codec(String),
    Clipboard(String),
    /// A frame was decoded in the page, `behind` milliseconds after its time
    /// on the device. A screen streamed as WebRTC tracks is counted from the
    /// `<video>` instead.
    Shown {
        behind: f64,
    },
    /// Bytes of video received.
    Received(usize),
    /// The reason, if it failed.
    Ended(Option<String>),
}

#[async_trait(?Send)]
pub(crate) trait Screen {
    fn features(&self) -> Features;

    async fn start(&self) -> Result<Session, String> {
        Err(unsupported("screen"))
    }

    /// Holds it at `to` degrees counter-clockwise from upright, or hands it
    /// back to the device's accelerometer with None.
    async fn turn(&self, _to: Option<u16>) -> Result<(), String> {
        Err(unsupported("rotation"))
    }

    async fn turned(&self) -> Option<Turned> {
        None
    }
}
