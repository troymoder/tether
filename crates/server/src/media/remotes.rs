use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use protocol::{RemoteSource, access, encode, own};
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, watch};

use super::Out;
use crate::app::App;
use crate::task::Stopped;

/// The bytes of preview pictures that may wait to go on one channel. Pictures
/// past that are dropped.
const PREVIEW_WINDOW: usize = 512 * 1024;

const PREVIEW_APART: Duration = Duration::from_millis(66);

/// Each feed runs here on its own, whether or not the page that started it is
/// still open. It ends when it is stopped or replaced, when a client feeds the
/// camera, or when the emulator stops.
#[derive(Default)]
pub(crate) struct Remotes(std::sync::Mutex<HashMap<String, Remote>>);

struct Remote {
    feed: protocol::RemoteFeed,
    /// Lets a feed that ends remove itself and leave one that replaced it.
    id: u64,
    framing: watch::Sender<protocol::Framing>,
    previews: Previews,
    _running: Stopped,
}

/// The newest preview picture of a feed, as an encoded
/// `protocol::Previewed::Picture`. Dropping it ends the pages' channels.
type Previews = watch::Sender<Option<Arc<Vec<u8>>>>;

impl Remotes {
    pub(crate) fn feed(&self, emulator: &str) -> Option<protocol::RemoteFeed> {
        self.0
            .lock()
            .unwrap()
            .get(emulator)
            .map(|remote| remote.feed.clone())
    }

    pub(crate) fn stop(&self, emulator: &str) -> bool {
        self.0.lock().unwrap().remove(emulator).is_some()
    }

    fn ended(&self, emulator: &str, id: u64) {
        let mut remotes = self.0.lock().unwrap();

        if remotes.get(emulator).is_some_and(|remote| remote.id == id) {
            remotes.remove(emulator);
        }
    }
}

pub(super) async fn start(
    app: &Arc<App>,
    client: &protocol::Client,
    emulator: &str,
    feed: protocol::RemoteFeed,
) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    if !app.emulators.runs(emulator) {
        return Err(format!("camera failed: {emulator} is not running"));
    }

    // Claimed as a remote feed, which makes this client's own stream give way.
    if let Err(by) = app
        .emulators
        .claim_camera(emulator, (&client.print, &client.name), true)
    {
        return Err(format!("camera failed: in use by {}", by.name));
    }

    let (size, capturing) = open(app, &feed.source).await?;
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let framing = watch::Sender::new(protocol::Framing::default());
    let previews = watch::Sender::new(None);
    let (started, starting) = tokio::sync::oneshot::channel();

    {
        // Held while the feed is spawned, so a feed that ends at once finds
        // itself in the map to remove.
        let mut remotes = app.remotes.0.lock().unwrap();
        let running = tokio::spawn(run(
            app.clone(),
            (emulator.to_owned(), feed.lens, id),
            size,
            capturing,
            (framing.subscribe(), previews.clone()),
            started,
        ));

        remotes.insert(
            emulator.to_owned(),
            Remote {
                feed,
                id,
                framing,
                previews,
                _running: Stopped(running.abort_handle()),
            },
        );
    }

    starting
        .await
        .unwrap_or_else(|_| Err("camera failed: stopped".to_owned()))
}

async fn open(
    app: &App,
    source: &RemoteSource,
) -> Result<((u16, u16), crate::webcam::Capturing), String> {
    match source {
        RemoteSource::Webcam {
            path,
            width,
            height,
            fps,
        } => {
            // Only a listed webcam, at a size and rate it gives, because these
            // go to ffmpeg. Listing blocks on each device.
            let webcams = tokio::task::spawn_blocking(crate::webcam::list)
                .await
                .map_err(|error| format!("webcam failed: {error}"))?;
            let size = webcams
                .iter()
                .find(|webcam| webcam.path == *path)
                .and_then(|webcam| {
                    webcam
                        .sizes
                        .iter()
                        .find(|size| {
                            (size.width, size.height) == (*width, *height)
                                && size.rates.contains(fps)
                        })
                        .cloned()
                })
                .ok_or_else(|| {
                    format!("webcam failed: {path} does not give {width}x{height} at {fps} fps")
                })?;

            let capturing = crate::webcam::capture(&app.ffmpeg, path, &size, *fps)
                .map_err(|error| format!("webcam failed: {error:#}"))?;

            Ok(((*width, *height), capturing))
        }
        RemoteSource::File { path } => {
            let file = crate::remote::whole(path)
                .map_err(|error| format!("camera file failed: {error:#}"))?;
            let picture = crate::remote::picture(&file)
                .ok_or_else(|| format!("camera file failed: not a video or a picture: {path}"))?;
            let size = crate::remote::size(&app.ffmpeg, &file)
                .await
                .map_err(|error| format!("camera file failed: {error:#}"))?;
            let playing = crate::webcam::play(&app.ffmpeg, &file, size, picture)
                .map_err(|error| format!("camera file failed: {error:#}"))?;

            Ok((size, playing))
        }
    }
}

async fn run(
    app: Arc<App>,
    (emulator, lens, id): (String, protocol::Lens, u64),
    (width, height): (u16, u16),
    capturing: crate::webcam::Capturing,
    (framing, previews): (watch::Receiver<protocol::Framing>, Previews),
    started: tokio::sync::oneshot::Sender<Result<(), String>>,
) {
    let mut started = Some(started);
    let width = usize::from(width);
    let height = usize::from(height);
    // Read on a task of its own, a whole frame at a time, because a read
    // cancelled half way would lose its place in the stream.
    let (frames, mut taken) = mpsc::channel::<Vec<u8>>(1);
    let mut output = capturing.output;
    let _reading = Stopped(
        tokio::spawn(async move {
            loop {
                let mut frame = vec![0; width * height * 3 / 2];

                if output.read_exact(&mut frame).await.is_err() || frames.send(frame).await.is_err()
                {
                    return;
                }
            }
        })
        .abort_handle(),
    );

    let mut previewed = std::time::Instant::now() - PREVIEW_APART;

    while let Some(frame) = taken.recv().await {
        if let Some(started) = started.take() {
            let _ = started.send(Ok(()));
        }

        // A client may feed the camera for a moment before it stops this feed.
        if let Some(fed) = app.emulators.feeding(&emulator, lens, None) {
            let framing = *framing.borrow();
            let made = tokio::task::block_in_place(|| {
                crate::frame::framed(
                    frame.clone(),
                    (width, height),
                    framing,
                    fed.size(),
                    fed.upright(),
                )
            });

            if let Some(made) = made {
                fed.raw(made);
            }
        } else if !app.emulators.runs(&emulator) {
            break;
        }

        if previews.receiver_count() > 0
            && previewed.elapsed() >= PREVIEW_APART
            && let Some(jpeg) =
                tokio::task::block_in_place(|| crate::webcam::preview(&frame, (width, height)))
        {
            previews.send_replace(Some(Arc::new(encode(&protocol::Previewed::Picture(jpeg)))));
            previewed = std::time::Instant::now();
        }
    }

    if let Some(started) = started {
        let _ = started.send(Err("camera failed: no picture came".to_owned()));
    }

    app.emulators.black_camera(&emulator);
    // Last, because this drops the handle of the task running it.
    app.remotes.ended(&emulator, id);
}

pub(super) async fn preview(
    app: &Arc<App>,
    out: &Out,
    emulator: &str,
    mut from: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let watched = app
        .remotes
        .0
        .lock()
        .unwrap()
        .get(emulator)
        .map(|remote| (remote.framing.clone(), remote.previews.subscribe()));
    let Some((framing, mut previews)) = watched else {
        out.answer(&encode(&Err::<protocol::Framing, String>(format!(
            "camera failed: nothing feeds {emulator} from tetherd"
        ))));

        return;
    };

    let mut framings = framing.subscribe();
    out.put(&encode(&Ok::<protocol::Framing, String>(
        *framings.borrow_and_update(),
    )));

    let window = Arc::new(tokio::sync::Semaphore::new(PREVIEW_WINDOW));
    // The framing this page sent last, which is not echoed back to it.
    let mut framed = None;

    loop {
        tokio::select! {
            changed = previews.changed() => {
                if changed.is_err() {
                    out.close();

                    return;
                }

                let jpeg = previews.borrow_and_update().clone();

                if let Some(jpeg) = jpeg {
                    out.put_paced(&jpeg, &window);
                }
            }
            changed = framings.changed() => {
                if changed.is_err() {
                    out.close();

                    return;
                }

                let now = *framings.borrow_and_update();

                if framed != Some(now) {
                    out.put(&encode(&protocol::Previewed::Framing(now)));
                }
            }
            came = from.recv() => {
                let Some(came) = came else {
                    return;
                };

                if let Ok(new) = access::<protocol::Framing>(&came) {
                    let new = own::<protocol::Framing>(new);
                    framed = Some(new);
                    framing.send_replace(new);
                }
            }
        }
    }
}
