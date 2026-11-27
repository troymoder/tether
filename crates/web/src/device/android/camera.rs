use android::scrcpy::{self, Input, control_message};
use async_trait::async_trait;
use futures_channel::mpsc;
use futures_util::StreamExt;
use leptos::task::spawn_local;
use protocol::{ArchivedScreen, Media, PhoneCamera, access};

use super::{Android, jar};
use crate::views::camera::{Camera, Control, Emulator, Kind, Mirrored};

/// The factor scrcpy zooms a camera by in one step.
const ZOOM_STEP: f32 = 1.0 + 1.0 / 16.0;

fn steps(zoom: f32) -> i32 {
    (zoom.ln() / ZOOM_STEP.ln()).round() as i32
}

#[async_trait(?Send)]
impl Camera for Android {
    /// An emulator shows nothing until tetherd has sent its settings, which
    /// list its cameras.
    fn kind(&self) -> Kind {
        if !self.id.is_emulator() {
            return Kind::Mirrored;
        }

        Emulator::of(self.link, &self.id).map_or(Kind::None, Kind::Fed)
    }

    async fn cameras(&self) -> Result<Vec<scrcpy::Camera>, String> {
        let jar = jar(self.link).await?;

        scrcpy::cameras(&self.adb().await, &jar)
            .await
            .map_err(|error| format!("camera failed: {error:#}"))
    }

    async fn mirror(&self, camera: PhoneCamera) -> Result<Mirrored, String> {
        let tracks = self.link.tracks().await?;
        let (video, audio) = tracks
            .mids()
            .ok_or_else(|| "camera failed: the tracks have no mids".to_owned())?;
        let mut step = steps(camera.zoom);
        let lane = self
            .link
            .lane(&Media::Screen {
                device: self.id.clone(),
                video,
                audio,
                camera: Some(camera),
            })
            .ok_or_else(|| "camera failed: not connected to the server".to_owned())?;
        let mut heard = lane.heard();
        let (control, mut controls) = mpsc::unbounded();

        // The lane lives as long as the view holds `control`.
        spawn_local(async move {
            while let Some(control) = controls.next().await {
                match control {
                    Control::Torch(on) => {
                        lane.send(&control_message(&Input::Torch(on)));
                    }
                    // scrcpy zooms one step at a time, and the camera picks
                    // the lens for the zoom reached.
                    Control::Zoom(to) => {
                        let to = steps(to);
                        let input = if to > step {
                            Input::ZoomIn
                        } else {
                            Input::ZoomOut
                        };

                        for _ in 0..(to - step).abs() {
                            lane.send(&control_message(&input));
                        }

                        step = to;
                    }
                }
            }
        });

        Ok(Mirrored {
            // A clone of the JS handle to the same stream. `MediaStream::clone`
            // would make a new stream with its own tracks.
            stream: Clone::clone(&tracks.stream),
            control,
            ended: Box::pin(async move {
                while let Some(message) = heard.next().await {
                    if let Ok(ArchivedScreen::Failed(why)) = access::<protocol::Screen>(&message) {
                        return why.replacen("screen failed", "camera failed", 1);
                    }
                }

                "camera failed: it stopped".to_owned()
            }),
            kept: Box::new(tracks),
        })
    }
}
