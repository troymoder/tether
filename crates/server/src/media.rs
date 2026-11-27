//! Each job that carries much data gets a DataChannel of its own, so the
//! control channel only carries small messages and never waits behind a picture
//! or a package. The page opens one and says in its first message what it is
//! for, and the job runs until either side closes it.

mod camera;
mod ios;
mod pair;
mod remotes;
mod screen;
mod watch;

use std::collections::HashMap;
use std::sync::Arc;

use protocol::{ArchivedMedia, DeviceId, Gather, InstanceId, Media, access, encode, own};
use str0m::channel::ChannelId;
use str0m::media::Mid;
use tokio::sync::mpsc;

use crate::app::App;
use crate::rtc::{Credit, Hub, Peer};

pub(crate) use camera::Streams;
pub(crate) use remotes::Remotes;
pub(crate) use screen::{Part, Watching, profile};

pub(crate) struct Lanes {
    app: Arc<App>,
    client: Arc<protocol::Client>,
    hub: Hub,
    peer: Peer,
    open: HashMap<ChannelId, Lane>,
    cameras: camera::Feeds,
    keys: screen::Keys,
}

struct Lane {
    gather: Gather,
    /// Dropping it as the channel closes ends the job.
    to: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

#[derive(Clone)]
struct Out {
    hub: Hub,
    peer: Peer,
    channel: ChannelId,
}

impl Out {
    fn put(&self, message: &[u8]) {
        for part in protocol::parts(message) {
            self.hub.write(self.peer, self.channel, part, None);
        }
    }

    /// Puts a message if `window` has room for it, and gives the room back
    /// once the message has gone. True if it was put.
    fn put_paced(&self, message: &[u8], window: &Arc<tokio::sync::Semaphore>) -> bool {
        let parts: Vec<Vec<u8>> = protocol::parts(message).collect();
        let size: usize = parts.iter().map(Vec::len).sum();
        let Ok(room) = u32::try_from(size)
            .map_err(|_| ())
            .and_then(|size| window.try_acquire_many(size).map_err(|_| ()))
        else {
            return false;
        };

        room.forget();
        for part in parts {
            let credit = Credit(window.clone(), part.len() as u32);
            self.hub.write(self.peer, self.channel, part, Some(credit));
        }

        true
    }

    /// Closes the channel once everything put on it has gone.
    fn close(&self) {
        self.hub.close(self.peer, self.channel);
    }

    fn answer(&self, message: &[u8]) {
        self.put(message);
        self.close();
    }
}

impl Lanes {
    pub(crate) fn new(app: Arc<App>, client: Arc<protocol::Client>, hub: Hub, peer: Peer) -> Self {
        Self {
            app,
            client,
            hub,
            peer,
            open: HashMap::new(),
            cameras: camera::Feeds::default(),
            keys: screen::Keys::default(),
        }
    }

    /// The browser lost part of the video on the track `mid` and asks for a
    /// key frame, without which nothing after the loss can be decoded.
    pub(crate) fn key_frame(&self, mid: Mid) {
        self.keys.ask(mid);
    }

    pub(crate) fn open(&mut self, channel: ChannelId) {
        self.open.insert(
            channel,
            Lane {
                gather: Gather::default(),
                to: None,
            },
        );
    }

    /// True if the channel is a media one.
    pub(crate) fn data(&mut self, channel: ChannelId, data: &[u8]) -> bool {
        let Some(lane) = self.open.get_mut(&channel) else {
            return false;
        };

        let Some(whole) = lane.gather.add(data) else {
            return true;
        };

        if let Some(to) = &lane.to {
            let _ = to.send(whole);

            return true;
        }

        let out = Out {
            hub: self.hub.clone(),
            peer: self.peer,
            channel,
        };

        if access::<Media>(&whole).is_err() {
            tracing::warn!("media channel closed: invalid first message");
            out.close();

            return true;
        }

        let (to, from) = mpsc::unbounded_channel();
        lane.to = Some(to);
        tokio::spawn(serve(
            (
                self.app.clone(),
                self.client.clone(),
                self.cameras.clone(),
                self.keys.clone(),
            ),
            out,
            whole,
            from,
        ));

        true
    }

    pub(crate) fn close(&mut self, channel: ChannelId) -> bool {
        self.open.remove(&channel).is_some()
    }
}

async fn serve(
    (app, client, cameras, keys): (Arc<App>, Arc<protocol::Client>, camera::Feeds, screen::Keys),
    out: Out,
    first: Vec<u8>,
    from: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    // `Lanes::data` checked it before spawning this.
    let Ok(media) = access::<Media>(&first) else {
        return;
    };

    match media {
        ArchivedMedia::Call { request } => {
            let result = crate::answer::noted(&app, request, &client).await;

            out.answer(&encode(&result));
        }
        ArchivedMedia::Webcams { instance } => {
            let webcams = if own::<InstanceId>(instance) == app.trust.primary() {
                tokio::task::spawn_blocking(crate::webcam::list)
                    .await
                    .map_err(|error| format!("webcams failed: {error}"))
            } else {
                Err("webcams failed: unknown instance".to_owned())
            };

            out.answer(&encode(&webcams));
        }
        ArchivedMedia::EmulatorOptions { instance } => {
            // shortcut: only the primary's own options, keep each instance's
            // under its id once linked instances report theirs.
            let options = if own::<InstanceId>(instance) == app.trust.primary() {
                app.emulators
                    .options()
                    .await
                    .cloned()
                    .ok_or_else(|| "emulator failed: no Android SDK".to_owned())
            } else {
                Err("emulator failed: unknown instance".to_owned())
            };

            out.answer(&encode(&options));
        }
        ArchivedMedia::ScrcpyServer => {
            let jar = tokio::fs::read(&app.scrcpy_server).await.map_err(|error| {
                format!("scrcpy failed: {}: {error}", app.scrcpy_server.display())
            });

            out.answer(&encode(&jar));
        }
        ArchivedMedia::AndroidHelper => {
            let dex = tokio::fs::read(&app.android_helper).await.map_err(|error| {
                format!("helper failed: {}: {error}", app.android_helper.display())
            });

            out.answer(&encode(&dex));
        }
        ArchivedMedia::Build { app: id, build } => {
            let package = app
                .apps
                .package(id, build.to_native())
                .await
                .map_err(|error| format!("{error:#}"));

            out.answer(&encode(&package));
        }
        ArchivedMedia::Watch { device } => watch::watch(&app, &out, &own(device), from).await,
        ArchivedMedia::Pair { name, password } => {
            pair::serve(&app, &out, (name.as_str(), password.as_str()), from).await;
        }
        ArchivedMedia::Ios { device, operation } => {
            let operation = own::<protocol::IosOperation>(operation);
            let device = own::<DeviceId>(device);

            ios::serve(&app, &out, &device.serial(), &operation, from).await;
        }
        ArchivedMedia::Screen {
            device,
            video,
            audio,
            camera,
        } => {
            let camera = camera.as_ref().map(own::<protocol::PhoneCamera>);

            screen::serve(
                &app,
                &out,
                &own(device),
                (video.as_str(), audio.as_str()),
                camera,
                (from, &keys),
            )
            .await;
        }
        ArchivedMedia::Camera {
            emulator,
            lens,
            lossless,
        } => {
            let lens = own::<protocol::Lens>(lens);

            camera::serve(
                &app,
                &client,
                &cameras,
                out,
                (emulator.as_str(), lens),
                *lossless,
                from,
            )
            .await;
        }
        ArchivedMedia::Remote { emulator, feed } => {
            let feed = own::<protocol::RemoteFeed>(feed);

            if let Err(said) = remotes::start(&app, &client, emulator, feed).await {
                out.answer(&encode(&Err::<protocol::Framing, String>(said)));

                return;
            }

            remotes::preview(&app, &out, emulator, from).await;
        }
        ArchivedMedia::Preview { emulator } => remotes::preview(&app, &out, emulator, from).await,
        ArchivedMedia::Look { path } => {
            let path = path.to_string();
            let looked = tokio::task::spawn_blocking(move || crate::remote::look(&path))
                .await
                .map_err(|error| format!("{error}"))
                .and_then(|looked| looked.map_err(|error| format!("{error:#}")));

            out.answer(&encode(&looked));
        }
        ArchivedMedia::Find { path, sought } => {
            let path = path.to_string();
            let sought = sought.to_string();
            let found = tokio::task::spawn_blocking(move || crate::remote::find(&path, &sought))
                .await
                .map_err(|error| format!("{error}"))
                .and_then(|found| found.map_err(|error| format!("{error:#}")));

            out.answer(&encode(&found));
        }
    }
}

#[cfg(test)]
mod tests {
    use protocol::Lens;

    use super::*;

    #[test]
    fn a_first_message_says_what_a_channel_is_for() {
        let asked = encode(&Media::Camera {
            emulator: "pixel".to_owned(),
            lens: Lens::Back,
            lossless: false,
        });

        assert!(matches!(
            access::<Media>(&asked),
            Ok(ArchivedMedia::Camera {
                lossless: false,
                ..
            })
        ));

        assert!(access::<Media>(b"not one").is_err());
    }
}
