//! A device's screen and sound go on a page's WebRTC tracks, from the capture
//! all pages watching that device share. The channel the page asked on carries
//! what is told besides the media, and for an Android device the page's input
//! as scrcpy's control messages. Closing it ends the sending.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use android::scrcpy;
use protocol::{DeviceId, PhoneCamera, Platform, Screen, encode};
use str0m::media::Mid;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc};

use super::Out;
use crate::app::App;
use crate::rtc::{Sample, Track};

#[derive(Clone)]
pub(crate) enum Part {
    Sample(Arc<Sample>),
    /// An encoded [`Screen`] for the page's channel, such as the codec.
    Told(Arc<Vec<u8>>),
    /// An encoded [`Screen`]. Nothing comes after this.
    Failed(Arc<Vec<u8>>),
}

pub(crate) struct Watching {
    /// The codec and a black frame while the screen is off, to send before
    /// anything from `parts`.
    pub(crate) codec: Option<Arc<Vec<u8>>>,
    pub(crate) kept: Option<Arc<Sample>>,
    pub(crate) parts: broadcast::Receiver<Part>,
    /// scrcpy's control messages, for a device that takes input that way.
    pub(crate) control: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

/// The `profile_idc` of a codec as WebCodecs names it, `avc1.PPCCLL`, which
/// picks the browser's H.264 payload type for it.
pub(crate) fn profile(codec: &str) -> u8 {
    codec
        .strip_prefix("avc1.")
        .and_then(|rest| rest.get(..2))
        .and_then(|profile| u8::from_str_radix(profile, 16).ok())
        .unwrap_or(0x64)
}

pub(super) async fn serve(
    app: &Arc<App>,
    out: &Out,
    device: &DeviceId,
    (video, audio): (&str, &str),
    camera: Option<PhoneCamera>,
    (mut from, keys): (mpsc::UnboundedReceiver<Vec<u8>>, &Keys),
) {
    let source = match camera {
        Some(camera) => scrcpy::Source::Camera {
            id: camera.id,
            mode: camera.mode.map(|mode| (mode.size, mode.rate)),
            zoom: camera.zoom,
        },
        None => scrcpy::Source::Screen,
    };
    let watched = match app.device(device) {
        Ok(_) if device.platform() == Platform::Ios => app.iphones.watch(&device.serial()),
        Ok(_) => app.screens.watch(app, &device.serial(), source),
        Err(error) => Err(error),
    };
    let watching = match watched {
        Ok(watching) => watching,
        Err(error) => {
            out.answer(&encode(&Screen::Failed(format!("{error:#}"))));

            return;
        }
    };
    let tracks = Tracks {
        video: Mid::from(video),
        audio: Mid::from(audio),
    };
    let control = watching.control.clone();

    if let Some(control) = &control {
        keys.0.lock().unwrap().insert(tracks.video, control.clone());
    }

    let heard = async {
        while let Some(message) = from.recv().await {
            if let Some(control) = &control {
                let _ = control.send(message);
            }
        }
    };

    tokio::select! {
        () = pass(out, &tracks, watching) => {}
        () = heard => {}
    }

    keys.0.lock().unwrap().remove(&tracks.video);
    out.close();
}

/// The places a page's video tracks ask for key frames: the control socket of
/// the scrcpy that sends to each, by mid. An iPhone's capture makes a key
/// frame only every so often by itself and has no way to be asked.
#[derive(Clone, Default)]
pub(crate) struct Keys(Arc<Mutex<HashMap<Mid, mpsc::UnboundedSender<Vec<u8>>>>>);

impl Keys {
    pub(crate) fn ask(&self, mid: Mid) {
        if let Some(control) = self.0.lock().unwrap().get(&mid) {
            let _ = control.send(scrcpy::control_message(&scrcpy::Input::ResetVideo));
        }
    }
}

struct Tracks {
    video: Mid,
    audio: Mid,
}

async fn pass(out: &Out, tracks: &Tracks, mut watching: Watching) {
    // True until a key frame went: frames before it cannot be decoded.
    let mut waiting = true;

    if let Some(codec) = watching.codec.take() {
        out.put(&codec);
    }

    if let Some(black) = watching.kept.take() {
        out.hub.media(out.peer, tracks.video, black);
        waiting = false;
    }

    loop {
        let part = match watching.parts.recv().await {
            Ok(part) => part,
            // Fallen behind: what was missed cannot be decoded without the
            // next key frame, which scrcpy makes when asked.
            Err(RecvError::Lagged(_)) => {
                waiting = true;

                if let Some(control) = &watching.control {
                    let _ = control.send(scrcpy::control_message(&scrcpy::Input::ResetVideo));
                }

                continue;
            }
            Err(RecvError::Closed) => return,
        };

        match part {
            Part::Sample(sample) => {
                let mid = match sample.track {
                    Track::Video { .. } => {
                        if waiting && !sample.key {
                            continue;
                        }

                        waiting = false;
                        tracks.video
                    }
                    Track::Audio => tracks.audio,
                };

                out.hub.media(out.peer, mid, sample);
            }
            Part::Told(message) => out.put(&message),
            Part::Failed(message) => {
                out.put(&message);

                return;
            }
        }
    }
}
