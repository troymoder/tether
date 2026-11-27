//! Written against the scrcpy version in `VERSION`. The jar must be that
//! version, because the server refuses a client of any other.

mod cameras;
mod control;
mod keys;
mod stream;

use std::fmt::Debug;
use std::time::Duration;

use adb::libadb::Session;
use adb::{Channel, Device};
use anyhow::{Result, bail, ensure};

pub use cameras::{Camera, Facing, cameras};
pub use control::{Button, Control, Input, TouchAction, control_message};
pub use stream::{Picture, Pictures, Replies, Sound};

use stream::Stream;

const VERSION: &str = "5.0.1";
const JAR: &str = "/data/local/tmp/scrcpy-server.jar";

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Screen,
    /// Needs Android 12 or later, and has no audio. Nothing else on the
    /// device can use that camera meanwhile. `mode` is a size and frame
    /// rate, or None to let scrcpy choose.
    Camera {
        id: String,
        mode: Option<(String, u32)>,
        zoom: f32,
    },
}

pub struct Screen<S> {
    pub pictures: Pictures<S>,
    pub sound: Option<Sound<S>>,
    pub replies: Replies<S>,
    pub control: Control<S>,
    pub server: adb::Output<S>,
    pub closer: Closer<S>,
}

/// `id` names the server's socket, so each viewer gets its own. `pause`
/// sleeps between connection attempts while the server starts.
pub async fn start<S, F>(
    device: &Device<S>,
    jar: &[u8],
    source: Source,
    id: u32,
    pause: impl Fn(Duration) -> F,
) -> Result<Screen<S>>
where
    S: Session + Clone,
    S::IoError: Debug,
    F: Future<Output = ()>,
{
    // Pushed every time, because the device may have been wiped since the last viewer.
    device.push(jar, JAR).await?;

    // scrcpy accepts eight hex digits below 2^31.
    let scid = format!("{:08x}", id >> 1);
    // CLASSPATH= stays outside the quoted arguments, because the shell would
    // read a quoted assignment as the program's name.
    let arguments = adb::command(server_arguments(&source, &scid))?;
    let command = format!("CLASSPATH={JAR} {arguments}");

    // The server serves one viewer and exits when its sockets close.
    let adb::Process { input, output } = device.spawn(&command, None).await?;
    let socket = format!("localabstract:scrcpy_{scid}");

    // The device refuses the socket until scrcpy listens on it, so this
    // retries for up to ten seconds. scrcpy sends one dummy byte on its first
    // connection.
    let mut video = None;

    for _ in 0..100 {
        if let Ok(channel) = device.channel(&socket).await {
            let mut stream = Stream::new(channel);

            if let Ok(Some(_)) = stream.exact(1).await {
                video = Some(stream);
                break;
            }
        }

        pause(Duration::from_millis(100)).await;
    }

    let Some(mut video) = video else {
        let _ = input.shut().await;

        bail!("scrcpy failed: no answer");
    };

    // A failure past here closes the open sockets, or scrcpy keeps running.
    let mut channels = vec![video.channel.handle()];
    let ready = async {
        // scrcpy takes the next connection as the audio socket when audio is
        // on, and the one after as the control socket.
        let sound = match source {
            Source::Screen => {
                let sound = Stream::new(device.channel(&socket).await?);

                channels.push(sound.channel.handle());

                Some(sound)
            }
            Source::Camera { .. } => None,
        };

        let control = device.channel(&socket).await?;

        channels.push(control.handle());

        // Skips the codec id, because the codec is read from the stream itself.
        ensure!(
            video.exact(4).await?.is_some(),
            "scrcpy failed: it has stopped"
        );

        anyhow::Ok((sound, control))
    }
    .await;

    let closer = Closer {
        channels,
        server: input,
    };

    let (sound, control) = match ready {
        Ok(ready) => ready,
        Err(error) => {
            let _ = closer.close().await;

            return Err(error);
        }
    };

    Ok(Screen {
        pictures: Pictures::new(video),
        sound: sound.map(Sound::new),
        replies: Replies::new(Stream::new(control.handle())),
        control: Control::new(control),
        server: output,
        closer,
    })
}

fn server_arguments(source: &Source, scid: &str) -> Vec<String> {
    let mut arguments = vec![
        "app_process".to_owned(),
        "/".to_owned(),
        "com.genymobile.scrcpy.Server".to_owned(),
        VERSION.to_owned(),
        format!("scid={scid}"),
        "log_level=info".to_owned(),
        "tunnel_forward=true".to_owned(),
        // Cleanup deletes the jar as soon as scrcpy starts, which would stop
        // the next viewer from starting one.
        "cleanup=false".to_owned(),
        "send_device_meta=false".to_owned(),
    ];

    match source {
        Source::Screen => arguments.extend([
            // The device may encode on its CPU, which cannot keep up with a
            // full-size phone screen.
            "max_size=1280".to_owned(),
            // Device audio in Opus, on Android 11 or later.
            "audio=true".to_owned(),
            "audio_codec=opus".to_owned(),
            "max_fps=60".to_owned(),
            "clipboard_autosync=true".to_owned(),
            "keep_active=true".to_owned(),
        ]),
        // The control socket carries torch and zoom.
        Source::Camera { id, mode, zoom } => {
            arguments.extend([
                "video_source=camera".to_owned(),
                format!("camera_id={id}"),
                format!("camera_zoom={zoom}"),
                "audio=false".to_owned(),
                "clipboard_autosync=false".to_owned(),
            ]);

            match mode {
                // Rates above 60 fps need high-speed capture, which only
                // some sizes support.
                Some((size, rate)) => arguments.extend([
                    format!("camera_size={size}"),
                    format!("camera_fps={rate}"),
                    format!("camera_high_speed={}", *rate > 60),
                ]),
                None => arguments.push("max_size=1280".to_owned()),
            }
        }
    }

    arguments
}

/// In degrees counter-clockwise from upright.
pub async fn rotation<S>(device: &Device<S>) -> Result<u16>
where
    S: Session + Clone,
    S::IoError: Debug,
{
    let dump = device.sh(["dumpsys", "window", "displays"]).await?;

    let Some(degrees) = rotation_in(&dump) else {
        bail!("rotation failed: not in dumpsys");
    };

    Ok(degrees)
}

pub async fn auto_rotates<S>(device: &Device<S>) -> Result<bool>
where
    S: Session + Clone,
    S::IoError: Debug,
{
    let said = device
        .sh(["settings", "get", "system", "accelerometer_rotation"])
        .await?;

    Ok(said.trim() == "1")
}

/// `degrees` is counter-clockwise from upright. `user_rotation` counts
/// quarter turns, as `ROTATION_N` does.
pub async fn hold_rotation<S>(device: &Device<S>, degrees: u16) -> Result<()>
where
    S: Session + Clone,
    S::IoError: Debug,
{
    let quarters = (degrees / 90 % 4).to_string();

    device
        .sh(["settings", "put", "system", "accelerometer_rotation", "0"])
        .await?;
    device
        .sh(["settings", "put", "system", "user_rotation", &quarters])
        .await?;

    Ok(())
}

pub async fn rotate_freely<S>(device: &Device<S>) -> Result<()>
where
    S: Session + Clone,
    S::IoError: Debug,
{
    device
        .sh(["settings", "put", "system", "accelerometer_rotation", "1"])
        .await?;

    Ok(())
}

/// `dumpsys window displays` has `mCurrentRotation=ROTATION_90`.
fn rotation_in(dump: &str) -> Option<u16> {
    let (_, rest) = dump.split_once("mCurrentRotation=ROTATION_")?;
    let degrees: String = rest.chars().take_while(char::is_ascii_digit).collect();

    degrees.parse().ok()
}

/// Closing the sockets and the server ends every pending read.
pub struct Closer<S> {
    channels: Vec<Channel<S>>,
    server: adb::Input<S>,
}

impl<S> Closer<S>
where
    S: Session,
    S::IoError: Debug,
{
    pub async fn close(self) -> Result<()> {
        let mut closed = Ok(());

        for channel in self.channels {
            closed = closed.and(channel.close().await);
        }

        closed.and(self.server.shut().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_which_way_the_screen_is_drawn() {
        let dump = "  mDisplayRotation:\n      mCurrentRotation=ROTATION_270\n    mUserRotation=ROTATION_0";

        assert_eq!(rotation_in(dump), Some(270));
        assert_eq!(rotation_in("nothing of the kind"), None);
    }

    #[test]
    fn asks_the_server_for_the_source() {
        let screen = server_arguments(&Source::Screen, "0000002a");

        assert!(screen.contains(&"scid=0000002a".to_owned()));
        assert!(screen.contains(&"audio=true".to_owned()));
        assert!(screen.contains(&"max_size=1280".to_owned()));

        let fast = Source::Camera {
            id: "0".to_owned(),
            mode: Some(("1920x1080".to_owned(), 120)),
            zoom: 2.0,
        };

        let camera = server_arguments(&fast, "0000002a");

        assert!(camera.contains(&"camera_zoom=2".to_owned()));
        assert!(camera.contains(&"camera_high_speed=true".to_owned()));
        assert!(!camera.contains(&"max_size=1280".to_owned()));
    }
}
