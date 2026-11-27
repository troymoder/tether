//! tetherd captures each device once and shares it with every page that shows
//! it. The device's channel carries the codec, the clipboard and failures, and
//! takes an Android device's input as scrcpy control messages.

use std::rc::Rc;
use std::time::Duration;

use futures_channel::{mpsc, oneshot};
use leptos::task::spawn_local;
use protocol::{ArchivedScreen, DeviceId, Media, access};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MediaStream, RtcRtpReceiver};

use crate::link::Link;
use crate::link::lane::Lane;
use crate::util::sleep;
use crate::views::screen::Told;

pub(super) struct Tracked {
    pub(super) stream: MediaStream,
    pub(super) lane: Rc<Lane>,
    pub(super) told: mpsc::UnboundedReceiver<Told>,
    /// Dropping it removes the tracks and closes the channel, which leaves
    /// the capture to any other pages still showing it.
    pub(super) kept: oneshot::Sender<()>,
}

pub(super) async fn tracked(link: Link, id: &DeviceId) -> Result<Tracked, String> {
    let tracks = link.tracks().await?;
    let (video, audio) = tracks
        .mids()
        .ok_or_else(|| "screen failed: the tracks have no mids".to_owned())?;
    let lane = link
        .lane(&Media::Screen {
            device: id.clone(),
            video,
            audio,
            camera: None,
        })
        .ok_or_else(|| "screen failed: not connected to the server".to_owned())?;
    let lane = Rc::new(lane);
    let stream = tracks.stream.clone();
    let (told, heard) = mpsc::unbounded();
    let (kept, left) = oneshot::channel::<()>();

    rate(tracks.video(), told.clone());

    let reading = lane.clone();

    spawn_local(async move {
        let mut left = left;
        let ended = loop {
            let said = std::pin::pin!(reading.next());
            let came = match futures_util::future::select(said, &mut left).await {
                futures_util::future::Either::Left((Some(came), _)) => came,
                _ => break None,
            };

            let heard = match access::<protocol::Screen>(&came) {
                Ok(ArchivedScreen::Codec(codec)) => Told::Codec(codec.to_string()),
                Ok(ArchivedScreen::Clipboard(text)) => Told::Clipboard(text.to_string()),
                Ok(ArchivedScreen::Failed(error)) => break Some(error.to_string()),
                Err(_) => break Some("screen failed: unreadable message".to_owned()),
            };

            let _ = told.unbounded_send(heard);
        };

        drop(tracks);
        let _ = told.unbounded_send(Told::Ended(ended));
    });

    Ok(Tracked {
        stream,
        lane,
        told: heard,
        kept,
    })
}

/// The RTP packets never reach the page, so the count comes from the
/// browser's stats.
fn rate(receiver: RtcRtpReceiver, told: mpsc::UnboundedSender<Told>) {
    spawn_local(async move {
        let mut last = None::<f64>;

        while !told.is_closed() {
            if let Some(total) = received(&receiver).await {
                let more = last.map_or(0.0, |last| (total - last).max(0.0));

                last = Some(total);

                let _ = told.unbounded_send(Told::Received(more as usize));
            }

            sleep(Duration::from_secs(1)).await;
        }
    });
}

async fn received(receiver: &RtcRtpReceiver) -> Option<f64> {
    let report: js_sys::Map = JsFuture::from(receiver.get_stats())
        .await
        .ok()?
        .unchecked_into();
    let mut bytes = None;

    report.for_each(&mut |stats, _| {
        let field = |name: &str| js_sys::Reflect::get(&stats, &name.into()).ok();

        if field("type").and_then(|kind| kind.as_string()).as_deref() == Some("inbound-rtp") {
            bytes = field("bytesReceived").and_then(|total| total.as_f64());
        }
    });

    bytes
}
