use std::collections::HashMap;
use std::sync::Arc;

use protocol::{CameraFrame, access, encode};
use tokio::sync::mpsc;

use super::Out;
use crate::app::App;

/// The number of a page's channels feeding each camera, by emulator and lens. The
/// page opens the next channel before it closes the one before, so the camera
/// is only released with the last one.
pub(super) type Feeds = Arc<std::sync::Mutex<HashMap<(String, usize), u32>>>;

/// The channels that feed emulators' cameras from browsers, by emulator. They
/// all belong to one client at a time, and are closed as another takes over.
#[derive(Default)]
pub(crate) struct Streams(std::sync::Mutex<HashMap<String, Vec<Stream>>>);

/// One channel: its id, the client's print, and the channel itself.
type Stream = (u64, String, Out);

impl Streams {
    fn add(&self, emulator: &str, print: &str, out: Out) -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        self.0
            .lock()
            .unwrap()
            .entry(emulator.to_owned())
            .or_default()
            .push((id, print.to_owned(), out));

        id
    }

    fn remove(&self, emulator: &str, id: u64) {
        if let Some(streams) = self.0.lock().unwrap().get_mut(emulator) {
            streams.retain(|(was, ..)| *was != id);
        }
    }

    pub(crate) fn taken(&self, emulator: &str, held: &protocol::CameraHeld) {
        let said = encode(held);

        if let Some(streams) = self.0.lock().unwrap().get_mut(emulator) {
            streams.retain(|(_, print, out)| {
                if *print == held.by.print {
                    return true;
                }

                out.answer(&said);

                false
            });
        }
    }
}

/// Dropping the last one for a camera releases the camera, which then shows
/// black.
struct Feeding {
    cameras: Feeds,
    key: (String, usize),
    app: Arc<App>,
    print: String,
}

impl Feeding {
    fn start(
        cameras: &Feeds,
        app: &Arc<App>,
        client: &protocol::Client,
        (emulator, lens): (&str, protocol::Lens),
    ) -> Self {
        let key = (emulator.to_owned(), lens.at());
        *cameras.lock().unwrap().entry(key.clone()).or_default() += 1;

        Self {
            cameras: cameras.clone(),
            key,
            app: app.clone(),
            print: client.print.clone(),
        }
    }
}

impl Drop for Feeding {
    fn drop(&mut self) {
        let last = {
            let mut cameras = self.cameras.lock().unwrap();
            let left = cameras.get_mut(&self.key).map_or(0, |count| {
                *count -= 1;
                *count
            });

            if left == 0 {
                cameras.remove(&self.key);
            }

            left == 0
        };

        if last {
            self.app
                .emulators
                .leave_camera(Some(&self.key.0), &self.print, true);
        }
    }
}

pub(super) async fn serve(
    app: &Arc<App>,
    client: &protocol::Client,
    cameras: &Feeds,
    out: Out,
    (emulator, lens): (&str, protocol::Lens),
    lossless: bool,
    from: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    if let Err(by) = app
        .emulators
        .claim_camera(emulator, (&client.print, &client.name), false)
    {
        out.answer(&encode(&protocol::CameraHeld { by, taken: false }));

        return;
    }

    // This client replaces whatever fed the camera from tetherd's machine.
    app.remotes.stop(emulator);
    let streaming = app.streams.add(emulator, &client.print, out);
    let _feeding = Feeding::start(cameras, app, client, (emulator, lens));

    feed(app, client, (emulator, lens), lossless, from).await;
    app.streams.remove(emulator, streaming);
}

async fn feed(
    app: &Arc<App>,
    client: &protocol::Client,
    (emulator, lens): (&str, protocol::Lens),
    lossless: bool,
    mut from: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let print = Some(client.print.as_str());
    let mut decoder: Option<crate::decode::Decoder> = None;
    // Lossless frames are packed as changes to the frame before.
    let mut before: Option<Vec<u8>> = None;

    while let Some(whole) = from.recv().await {
        let Ok(frame) = access::<CameraFrame>(&whole) else {
            break;
        };

        let size = (
            usize::from(frame.width.to_native()),
            usize::from(frame.height.to_native()),
        );

        // None while another client feeds it or the emulator is not running.
        let Some(fed) = app.emulators.feeding(emulator, lens, print) else {
            continue;
        };

        if lossless {
            let unpacked = (size == fed.size())
                .then(|| protocol::unpack(&frame.data, before.as_deref(), size.0, size.1))
                .flatten();

            if let Some(unpacked) = unpacked {
                fed.raw(unpacked.clone());
                before = Some(unpacked);
            }

            continue;
        }

        if decoder
            .as_ref()
            .is_none_or(|decoder| !decoder.feeds(&fed, size))
        {
            decoder = match crate::decode::Decoder::start(&app.ffmpeg, fed, size) {
                Ok(started) => Some(started),
                Err(error) => {
                    tracing::error!("camera failed: {error:#}");
                    break;
                }
            };
        }

        if let Some(decoder) = &mut decoder {
            decoder.give(&frame.data).await;
        }
    }
}
