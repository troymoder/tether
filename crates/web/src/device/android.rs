//! The screen comes from tetherd's scrcpy as WebRTC tracks (tracked.rs).
//! tetherd cannot reach a phone on this browser's WebUSB, so the page runs
//! scrcpy on it and decodes its video and sound into generated tracks.

use std::rc::Rc;

use android::scrcpy::{self, Picture, control_message};
use async_trait::async_trait;
use futures_channel::mpsc;
use futures_util::StreamExt;
use leptos::prelude::*;
use leptos::task::spawn_local;
use protocol::{DeviceId, Media, access, own};
use web_sys::{
    MediaStream, MediaStreamTrackGenerator, MediaStreamTrackGeneratorInit, VideoFrame,
    WritableStreamDefaultWriter,
};

use super::player::Player;
use super::speaker::Speaker;
use super::tracked::tracked;
use super::{Device, Views};
use crate::link::Link;
use crate::link::usb::Phone;
use crate::util::sleep;
use crate::views::screen::{self, Button, Input, Screen, Session, Told, TouchAction, Turned};

mod apps;
mod camera;
mod clipboard;
mod crashes;
mod data;
mod files;
pub(super) mod helper;
mod location;
mod logs;
mod media;
mod shell;
mod tweaks;

#[derive(Clone)]
pub(super) struct Android {
    link: Link,
    id: DeviceId,
}

impl Android {
    pub(super) fn new(link: Link, id: DeviceId) -> Self {
        Self { link, id }
    }

    async fn adb(&self) -> adb::Device<Phone> {
        self.link.adb(&self.id).await
    }

    async fn rooted(&self) -> bool {
        self.link.rooted(&self.id).await
    }
}

impl Device for Android {
    fn views(&self) -> Signal<Views> {
        Signal::stored(Views::all())
    }
}

#[async_trait(?Send)]
impl Screen for Android {
    fn features(&self) -> screen::Features {
        screen::Features::all()
    }

    async fn start(&self) -> Result<Session, String> {
        if self.link.held(&self.id.serial()).is_some() {
            decoded(self.link, &self.id).await
        } else {
            tethered(self.link, &self.id).await
        }
    }

    async fn turn(&self, to: Option<u16>) -> Result<(), String> {
        let device = self.adb().await;
        let done = match to {
            Some(to) => scrcpy::hold_rotation(&device, to).await,
            None => scrcpy::rotate_freely(&device).await,
        };

        done.map_err(|error| format!("{error:#}"))
    }

    async fn turned(&self) -> Option<Turned> {
        let device = self.adb().await;
        let degrees = scrcpy::rotation(&device).await.ok()? % 360;
        let auto = scrcpy::auto_rotates(&device).await.ok();

        Some(Turned { degrees, auto })
    }
}

async fn tethered(link: Link, id: &DeviceId) -> Result<Session, String> {
    let tracked = tracked(link, id).await?;
    let (input, mut inputs) = mpsc::unbounded::<Input>();
    let lane = tracked.lane.clone();

    spawn_local(async move {
        while let Some(input) = inputs.next().await {
            if !lane.send(&control_message(&scrcpy_input(input))) {
                break;
            }
        }
    });

    Ok(Session {
        stream: tracked.stream,
        input,
        told: tracked.told,
        kept: Box::new((tracked.kept, tracked.lane)),
    })
}

/// Dropping it closes scrcpy's sockets and server, which ends every task
/// reading them, and ends the generated tracks.
struct Decoding {
    closer: Option<scrcpy::Closer<Phone>>,
    writer: Option<WritableStreamDefaultWriter>,
    speaker: Option<Rc<Speaker>>,
}

impl Drop for Decoding {
    fn drop(&mut self) {
        if let Some(closer) = self.closer.take() {
            spawn_local(async move {
                let _ = closer.close().await;
            });
        }

        if let Some(writer) = self.writer.take() {
            let _ = writer.close();
        }
    }
}

/// `MediaStreamTrackGenerator` is Chromium only, which is fine because WebUSB
/// is too.
async fn decoded(link: Link, id: &DeviceId) -> Result<Session, String> {
    let device = link.adb(id).await;
    let jar = jar(link).await?;
    // A random socket name so it does not clash with another page's scrcpy.
    let socket = (js_sys::Math::random() * f64::from(u32::MAX)) as u32;
    let screen = scrcpy::start(&device, &jar, scrcpy::Source::Screen, socket, sleep)
        .await
        .map_err(|error| format!("{error:#}"))?;
    let scrcpy::Screen {
        mut pictures,
        sound,
        mut replies,
        mut control,
        mut server,
        closer,
    } = screen;
    // Dropping this closes scrcpy if anything below fails.
    let mut kept = Decoding {
        closer: Some(closer),
        writer: None,
        speaker: None,
    };
    let generator = MediaStreamTrackGenerator::new(&MediaStreamTrackGeneratorInit::new("video"))
        .map_err(|_| "screen failed: no track generator in this browser".to_owned())?;
    let writer = generator
        .writable()
        .get_writer()
        .map_err(|_| "screen failed: no track writer".to_owned())?;
    let speaker =
        Rc::new(Speaker::start().ok_or_else(|| "screen failed: no sound decoder".to_owned())?);
    let stream = MediaStream::new_with_tracks(&js_sys::Array::of2(&generator, &speaker.track))
        .map_err(|_| "screen failed: no stream".to_owned())?;
    let (told, heard) = mpsc::unbounded();
    let (input, mut inputs) = mpsc::unbounded::<Input>();

    kept.writer = Some(writer.clone());
    kept.speaker = Some(speaker.clone());

    let mut player = Player::new(
        {
            let told = told.clone();

            move |frame: VideoFrame| {
                let behind = js_sys::Date::now() - frame.timestamp() / 1000.0;

                // The track takes ownership of the frame and closes it.
                let _ = writer.write_with_chunk(&frame);
                let _ = told.unbounded_send(Told::Shown { behind });
            }
        },
        {
            let told = told.clone();

            move |_| {
                let failed = "screen failed: the video could not be decoded".to_owned();
                let _ = told.unbounded_send(Told::Ended(Some(failed)));
            }
        },
    );

    spawn_local(async move {
        while let Some(input) = inputs.next().await {
            if control.send(&scrcpy_input(input)).await.is_err() {
                break;
            }
        }
    });

    // The screen source asks for sound, but a device may have none to give.
    if let Some(mut sound) = sound {
        spawn_local(async move {
            while let Ok(Some((time, data))) = sound.next().await {
                speaker.decode(time, &data);
            }
        });
    }

    {
        let told = told.clone();

        spawn_local(async move {
            while let Ok(Some(text)) = replies.next().await {
                let _ = told.unbounded_send(Told::Clipboard(text));
            }

            let _ = told.unbounded_send(Told::Ended(None));
        });
    }

    {
        let told = told.clone();

        spawn_local(async move {
            while let Ok(Some(_)) = server.next().await {}

            let _ = told.unbounded_send(Told::Ended(None));
        });
    }

    spawn_local(async move {
        while let Ok(Some(picture)) = pictures.next().await {
            match picture {
                Picture::Codec(name) => {
                    player.start(&name);

                    let _ = told.unbounded_send(Told::Codec(name));
                }
                Picture::Frame { key, time, data } => {
                    let _ = told.unbounded_send(Told::Received(data.len()));

                    player.decode(key, time, &data);
                }
            }
        }

        player.close();
        let _ = told.unbounded_send(Told::Ended(None));
    });

    Ok(Session {
        stream,
        input,
        told: heard,
        kept: Box::new(kept),
    })
}

fn scrcpy_input(input: Input) -> scrcpy::Input {
    match input {
        Input::Touch {
            action,
            pointer,
            x,
            y,
            width,
            height,
        } => scrcpy::Input::Touch {
            action: match action {
                TouchAction::Down => scrcpy::TouchAction::Down,
                TouchAction::Move => scrcpy::TouchAction::Move,
                TouchAction::Up => scrcpy::TouchAction::Up,
                TouchAction::Cancel => scrcpy::TouchAction::Cancel,
            },
            pointer,
            x,
            y,
            width,
            height,
        },
        Input::Key {
            down,
            code,
            shift,
            ctrl,
            alt,
        } => scrcpy::Input::Key {
            down,
            code,
            shift,
            ctrl,
            alt,
        },
        Input::Text(text) => scrcpy::Input::Text(text),
        Input::Scroll {
            x,
            y,
            width,
            height,
            dx,
            dy,
        } => scrcpy::Input::Scroll {
            x,
            y,
            width,
            height,
            dx,
            dy,
        },
        Input::Button(button) => scrcpy::Input::Button(scrcpy_button(button)),
        Input::Hold {
            button,
            down,
            repeat,
        } => scrcpy::Input::Hold {
            button: scrcpy_button(button),
            down,
            repeat,
        },
        Input::ReadClipboard => scrcpy::Input::ReadClipboard,
        Input::Clipboard(text) => scrcpy::Input::Clipboard(text),
    }
}

fn scrcpy_button(button: Button) -> scrcpy::Button {
    match button {
        Button::Back => scrcpy::Button::Back,
        Button::Home => scrcpy::Button::Home,
        Button::Recents => scrcpy::Button::Recents,
        Button::Power => scrcpy::Button::Power,
        Button::VolumeUp => scrcpy::Button::VolumeUp,
        Button::VolumeDown => scrcpy::Button::VolumeDown,
    }
}

pub(crate) async fn jar(link: Link) -> Result<Vec<u8>, String> {
    fetched(link, &Media::ScrcpyServer, "scrcpy").await
}

pub(crate) async fn dex(link: Link) -> Result<Vec<u8>, String> {
    fetched(link, &Media::AndroidHelper, "helper").await
}

async fn fetched(link: Link, media: &Media, what: &str) -> Result<Vec<u8>, String> {
    let Some(lane) = link.lane(media) else {
        return Err(format!("{what} failed: not connected to the server"));
    };

    let Some(answer) = lane.next().await else {
        return Err(format!("{what} failed: connection to the server lost"));
    };

    access::<Result<Vec<u8>, String>>(&answer)
        .map(own::<Result<Vec<u8>, String>>)
        .map_err(|error| format!("{what} failed: {error}"))?
}
