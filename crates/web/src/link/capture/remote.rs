//! tetherd runs the feed on its own. While the page is open it previews the
//! feed on a canvas whose stream the Camera view plays.

use std::rc::Rc;

use futures_channel::oneshot;
use futures_util::future::{Either, select};
use js_sys::{Array, Uint8Array};
use leptos::prelude::*;
use protocol::{Framing, InstanceId, Lens, Media, RemoteFeed, RemoteSource, Request, access, own};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, CanvasRenderingContext2d, HtmlCanvasElement, ImageBitmap, MediaStream};

use super::{Feed, Feeding};
use crate::link::Link;
use crate::link::lane::Lane;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Remote {
    Webcam {
        path: String,
        size: (u16, u16),
        fps: u16,
    },

    File {
        path: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Remoting {
    pub lens: Lens,
    pub what: Remote,
}

pub(super) struct Preview {
    pub(super) lane: Rc<Lane>,
    pub(super) stream: MediaStream,
    /// Stops the drawing loop when dropped.
    _stop: oneshot::Sender<()>,
}

impl Remoting {
    pub(crate) fn of(feed: &RemoteFeed) -> Self {
        Self {
            lens: feed.lens,
            what: match &feed.source {
                RemoteSource::Webcam {
                    path,
                    width,
                    height,
                    fps,
                } => Remote::Webcam {
                    path: path.clone(),
                    size: (*width, *height),
                    fps: *fps,
                },
                RemoteSource::File { path } => Remote::File { path: path.clone() },
            },
        }
    }
}

pub(super) fn start_remote(feed: Feed, wanted: Remoting) -> Result<(), String> {
    let emulator = feed.emulator()?;
    let source = match wanted.what.clone() {
        Remote::Webcam { path, size, fps } => RemoteSource::Webcam {
            path,
            width: size.0,
            height: size.1,
            fps,
        },
        Remote::File { path } => RemoteSource::File { path },
    };
    let asked = Media::Remote {
        emulator,
        feed: RemoteFeed {
            lens: wanted.lens,
            source,
        },
    };

    preview(feed, wanted, &asked, true)
}

pub(super) fn watch_remote(feed: Feed, wanted: Remoting) {
    let Ok(emulator) = feed.emulator() else {
        return;
    };

    let _ = preview(feed, wanted, &Media::Preview { emulator }, false);
}

pub(super) fn stop_remote(feed: Feed) {
    let link = feed.link;

    let Ok(emulator) = feed.emulator() else {
        return;
    };

    leptos::task::spawn_local(async move {
        if let Err(error) = link.call(Request::StopRemote { emulator }).await {
            link.did("camera", feed.device().as_ref(), Err(error));
        }
    });
}

fn preview(feed: Feed, wanted: Remoting, asked: &Media, starting: bool) -> Result<(), String> {
    let link = feed.link;

    feed.stop();
    feed.frame(Framing::default());

    let canvas: HtmlCanvasElement = document()
        .create_element("canvas")
        .map_err(|_| "camera failed: no canvas")?
        .unchecked_into();
    let stream = canvas
        .capture_stream()
        .map_err(|_| "camera failed: no stream")?;
    let lane = Rc::new(
        link.lane(asked)
            .ok_or("camera failed: not connected to the server")?,
    );
    let (stop_drawing, mut stopped) = oneshot::channel::<()>();
    let reading = lane.clone();

    leptos::task::spawn_local(async move {
        // The first message is the start result, with the source's framing.
        let mut first = true;

        loop {
            let came = match select(std::pin::pin!(reading.next()), &mut stopped).await {
                Either::Left((Some(came), _)) => came,
                Either::Right(_) => return,
                Either::Left((None, _)) => break,
            };

            if std::mem::take(&mut first) {
                let started = access::<Result<Framing, String>>(&came)
                    .map(own::<Result<Framing, String>>)
                    .unwrap_or_else(|error| Err(format!("camera failed: {error}")));

                match started {
                    Ok(framing) => {
                        feed.framing.try_set(framing);
                    }
                    Err(said) => {
                        if starting {
                            link.did("camera", feed.device().as_ref(), Err(said));
                        }

                        break;
                    }
                }

                continue;
            }

            match access::<protocol::Previewed>(&came) {
                Ok(protocol::ArchivedPreviewed::Picture(jpeg)) => draw_jpeg(&canvas, jpeg).await,
                // Another page changed the framing, so it changes here too.
                Ok(protocol::ArchivedPreviewed::Framing(framing)) => {
                    feed.framing.try_set(own::<Framing>(framing));
                }
                Err(_) => {}
            }
        }

        // tetherd stopped the feed. Nothing is stopped here if another
        // source took its place meanwhile.
        let current = feed.feeding.try_with_untracked(|feeding| {
            matches!(
                feeding,
                Feeding::Remote { preview, .. } if Rc::ptr_eq(&preview.lane, &reading)
            )
        });

        if current == Some(true) {
            feed.stop();
        }
    });

    feed.feed_from(Feeding::Remote {
        remoting: wanted,
        preview: Preview {
            lane,
            stream,
            _stop: stop_drawing,
        },
    });

    Ok(())
}

async fn ask(link: Link, what: &str, asked: &Media) -> Result<Vec<u8>, String> {
    let lane = link
        .lane(asked)
        .ok_or_else(|| format!("{what} failed: not connected to the server"))?;

    lane.next()
        .await
        .ok_or_else(|| format!("{what} failed: connection to the server lost"))
}

pub(crate) async fn look(link: Link, path: String) -> Result<protocol::Looked, String> {
    let came = ask(link, "look", &Media::Look { path }).await?;

    access::<Result<protocol::Looked, String>>(&came)
        .map(own::<Result<protocol::Looked, String>>)
        .map_err(|error| format!("look failed: {error}"))?
}

pub(crate) async fn find(
    link: Link,
    path: String,
    sought: String,
) -> Result<Vec<protocol::Match>, String> {
    let came = ask(link, "find", &Media::Find { path, sought }).await?;

    access::<Result<Vec<protocol::Match>, String>>(&came)
        .map(own::<Result<Vec<protocol::Match>, String>>)
        .map_err(|error| format!("find failed: {error}"))?
}

pub(crate) async fn webcams(
    link: Link,
    instance: InstanceId,
) -> Result<Vec<protocol::Webcam>, String> {
    let came = ask(link, "webcams", &Media::Webcams { instance }).await?;

    access::<Result<Vec<protocol::Webcam>, String>>(&came)
        .map(own::<Result<Vec<protocol::Webcam>, String>>)
        .map_err(|error| format!("webcams failed: {error}"))?
}

pub(crate) async fn emulator_options(
    link: Link,
    instance: InstanceId,
) -> Result<protocol::EmulatorOptions, String> {
    let came = ask(link, "emulator", &Media::EmulatorOptions { instance }).await?;

    access::<Result<protocol::EmulatorOptions, String>>(&came)
        .map(own::<Result<protocol::EmulatorOptions, String>>)
        .map_err(|error| format!("emulator failed: {error}"))?
}

async fn draw_jpeg(canvas: &HtmlCanvasElement, jpeg: &[u8]) {
    let bytes = Array::of1(&Uint8Array::from(jpeg));

    let Ok(blob) = Blob::new_with_u8_array_sequence(&bytes) else {
        return;
    };

    let Ok(decoding) = window().create_image_bitmap_with_blob(&blob) else {
        return;
    };

    let Ok(bitmap) = JsFuture::from(decoding).await else {
        return;
    };

    let bitmap: ImageBitmap = bitmap.unchecked_into();

    if canvas.width() != bitmap.width() || canvas.height() != bitmap.height() {
        canvas.set_width(bitmap.width());
        canvas.set_height(bitmap.height());
    }

    if let Ok(Some(pen)) = canvas.get_context("2d") {
        let pen: CanvasRenderingContext2d = pen.unchecked_into();
        let _ = pen.draw_image_with_image_bitmap(&bitmap, 0.0, 0.0);
    }

    bitmap.close();
}
