use js_sys::{Array, Reflect};
use leptos::prelude::*;
use protocol::Lens;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    DisplayMediaStreamConstraints, HtmlImageElement, HtmlVideoElement, MediaDeviceInfo,
    MediaDeviceKind, MediaStream, MediaStreamConstraints, MediaStreamTrack, Url,
};

use super::running::{Player, run};
use super::{Feed, Feeding, object};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Wanted {
    pub lens: Lens,
    pub what: Capture,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Capture {
    Screen,
    Camera {
        id: Option<String>,
        /// The ideal size. The browser gives the nearest it supports.
        size: (u32, u32),
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Played {
    pub lens: Lens,
    pub url: String,
    pub image: bool,
    pub name: String,
}

pub(crate) const SIZES: [(u32, u32); 4] = [(640, 480), (1280, 720), (1920, 1080), (3840, 2160)];

pub(super) fn release_stream(stream: &MediaStream) {
    for track in stream.get_tracks() {
        track.unchecked_into::<MediaStreamTrack>().stop();
    }
}

pub(super) fn release_file(url: &str) {
    let _ = Url::revoke_object_url(url);
}

/// This browser's cameras, as id and name. The names are empty until the
/// user has let the page use a camera once.
pub(crate) async fn cameras() -> Vec<(String, String)> {
    let Ok(devices) = window().navigator().media_devices() else {
        return Vec::new();
    };

    let Ok(listing) = devices.enumerate_devices() else {
        return Vec::new();
    };

    JsFuture::from(listing)
        .await
        .map(|listed| {
            Array::from(&listed)
                .iter()
                .map(JsCast::unchecked_into::<MediaDeviceInfo>)
                .filter(|device| device.kind() == MediaDeviceKind::Videoinput)
                .map(|device| (device.device_id(), device.label()))
                .collect()
        })
        .unwrap_or_default()
}

fn said(error: JsValue) -> String {
    let name = Reflect::get(&error, &"name".into())
        .ok()
        .and_then(|name| name.as_string());

    match name.as_deref() {
        Some("NotAllowedError") => "capture failed: not allowed".to_owned(),
        Some("NotFoundError") => "capture failed: no such camera".to_owned(),
        Some(name) => format!("capture failed: {name}"),
        None => "capture failed".to_owned(),
    }
}

async fn ask(wanted: &Wanted) -> Result<MediaStream, String> {
    let devices = window()
        .navigator()
        .media_devices()
        .map_err(|_| "capture unavailable: unsupported browser or page not on localhost")?;

    let asked = match &wanted.what {
        Capture::Screen => {
            let constraints = DisplayMediaStreamConstraints::new();

            constraints.set_video(&true.into());
            devices.get_display_media_with_constraints(&constraints)
        }
        Capture::Camera { id, size } => {
            let video = object(&[
                ("width", object(&[("ideal", size.0.into())]).into()),
                ("height", object(&[("ideal", size.1.into())]).into()),
            ]);

            if let Some(id) = id {
                let exactly = object(&[("exact", id.as_str().into())]);
                let _ = Reflect::set(&video, &"deviceId".into(), &exactly);
            }

            let constraints = MediaStreamConstraints::new();

            constraints.set_video(&video);
            devices.get_user_media_with_constraints(&constraints)
        }
    };

    let stream = JsFuture::from(asked.map_err(said)?).await.map_err(said)?;

    Ok(stream.unchecked_into())
}

fn video(play: impl FnOnce(&HtmlVideoElement)) -> Result<HtmlVideoElement, String> {
    let video: HtmlVideoElement = document()
        .create_element("video")
        .map_err(|_| "capture failed: no video element")?
        .unchecked_into();

    video.set_muted(true);
    play(&video);

    // `play` rejects when the video is stopped right away, which is harmless.
    if let Ok(started) = video.play() {
        leptos::task::spawn_local(async move {
            let _ = JsFuture::from(started).await;
        });
    }

    Ok(video)
}

pub(super) async fn start(feed: Feed, wanted: Wanted) -> Result<(), String> {
    let mine = feed.turn();
    let stream = ask(&wanted).await?;

    // The feed was stopped or restarted while the browser asked, or it is
    // gone with its device.
    if mine.is_none() || feed.turns.try_get_value() != mine {
        release_stream(&stream);

        return Ok(());
    }

    // Started only after the browser granted the stream, so the previous
    // source keeps going if the user declines.
    let started = video(|video| video.set_src_object(Some(&stream)))
        .and_then(|player| run(feed, wanted.lens, Player::Video(player), Some(&stream)));

    match started {
        Ok(running) => feed.feed_from(Feeding::Capturing {
            wanted,
            stream,
            running,
        }),
        Err(error) => {
            release_stream(&stream);

            return Err(error);
        }
    }

    Ok(())
}

pub(super) fn show(feed: Feed, lens: Lens, file: &web_sys::File) -> Result<(), String> {
    let url =
        Url::create_object_url_with_blob(file).map_err(|_| "camera file failed: unreadable")?;
    let image = file.type_().starts_with("image/");
    let player = if image {
        HtmlImageElement::new()
            .map(|image| {
                image.set_src(&url);

                Player::Image(image)
            })
            .map_err(|_| "camera file failed: no image element".to_owned())
    } else {
        video(|video| {
            video.set_loop(true);
            video.set_src(&url);
        })
        .map(Player::Video)
    };

    match player.and_then(|player| run(feed, lens, player, None)) {
        Ok(running) => {
            let played = Played {
                lens,
                url,
                image,
                name: file.name(),
            };

            feed.feed_from(Feeding::Playing { played, running });

            Ok(())
        }
        Err(error) => {
            release_file(&url);

            Err(error)
        }
    }
}
