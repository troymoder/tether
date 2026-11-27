//! Android is not up while an emulator boots or stops, so scrcpy cannot show
//! its screen. The emulator's own gRPC service gives pictures of its display
//! from the moment it draws.
//!
//! The service is started with `-grpc-use-token`, which has it listen on this
//! machine only and ask for a token. The emulator writes the token to a file
//! named after its process.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use protocol::{Picture, encode, squeeze};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;

use control::ImageFormat;
use control::emulator_controller_client::EmulatorControllerClient;
use control::image_format::ImgFormat;

mod control {
    #![allow(clippy::all, clippy::pedantic, unreachable_pub)]

    tonic::include_proto!("android.emulation.control");
}

/// Enough to watch a boot by and small to send.
const LARGEST: u32 = 800;

/// About 30 pictures a second.
const APART: Duration = Duration::from_millis(33);

/// The emulator's own convention is 8554 for the console on 5554.
pub(crate) fn port(console: u16) -> u16 {
    console + 3000
}

// shortcut: only the folders the emulator was seen to write it to on Linux
// and macOS, read the emulator's own discovery code if another system needs it.
fn token(pid: u32) -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let folders = [
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        home.as_ref()
            .map(|home| home.join("Library/Caches/TemporaryItems")),
        home.as_ref().map(|home| home.join(".android")),
    ];

    folders.into_iter().flatten().find_map(|folder| {
        let file = folder.join(format!("avd/running/pid_{pid}.ini"));
        let said = std::fs::read_to_string(file).ok()?;

        said.lines()
            .find_map(|line| line.strip_prefix("grpc.token="))
            .map(str::to_owned)
    })
}

pub(crate) async fn show(
    port: u16,
    pid: u32,
    out: &mpsc::Sender<Vec<u8>>,
    still: impl Fn() -> bool,
) {
    while still() {
        // The service comes up late, the token is written a moment after, and
        // there is no picture before the emulator has drawn one. All of these
        // fail the same way, so it retries.
        if pictures(port, pid, out, &still).await.is_ok() {
            return;
        }

        sleep(Duration::from_millis(300)).await;
    }
}

async fn pictures(
    port: u16,
    pid: u32,
    out: &mpsc::Sender<Vec<u8>>,
    still: &impl Fn() -> bool,
) -> Result<()> {
    let token = token(pid).context("no token yet")?;
    let channel = Channel::from_shared(format!("http://127.0.0.1:{port}"))?
        .connect()
        .await?;
    let bearer: MetadataValue<_> = format!("Bearer {token}").parse()?;
    let mut service = EmulatorControllerClient::with_interceptor(
        channel,
        move |mut request: tonic::Request<()>| {
            request
                .metadata_mut()
                .insert("authorization", bearer.clone());

            Ok(request)
        },
    );

    let mut stream = service
        .stream_screenshot(ImageFormat {
            format: ImgFormat::Rgba8888.into(),
            width: LARGEST,
            height: LARGEST,
            ..Default::default()
        })
        .await?
        .into_inner();
    let mut passed_on = Instant::now() - APART;

    while let Some(picture) = stream.message().await? {
        if !still() {
            return Ok(());
        }

        let Some(format) = picture.format else {
            continue;
        };

        let Ok(width) = u16::try_from(format.width) else {
            continue;
        };

        let Ok(height) = u16::try_from(format.height) else {
            continue;
        };

        if picture.image.len() != usize::from(width) * usize::from(height) * 4
            || passed_on.elapsed() < APART
        {
            continue;
        }

        passed_on = Instant::now();

        let message = encode(&Picture {
            width,
            height,
            data: squeeze(&picture.image),
        });

        if out.send(message).await.is_err() {
            return Ok(());
        }
    }

    anyhow::bail!("the pictures stopped")
}
