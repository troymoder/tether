use async_trait::async_trait;
use futures_channel::mpsc;
use futures_util::StreamExt;
use ios::input::{InterfaceOrientation, Orientation};
use leptos::prelude::*;
use leptos::task::spawn_local;
use protocol::DeviceId;

use super::tracked::tracked;
use super::{Device, Views};
use crate::link::Link;
use crate::views::apps::Apps;
use crate::views::camera::Camera;
use crate::views::clipboard::Clipboard;
use crate::views::crashes::Crashes;
use crate::views::data::Data;
use crate::views::location::Location;
use crate::views::logs::Logs;
use crate::views::preferences::Tweaks;
use crate::views::screen::{self, Button, Input, Screen, Session, TouchAction};
use crate::views::shell::Shell;

#[derive(Clone)]
pub(super) struct Iphone {
    link: Link,
    id: DeviceId,
}

impl Iphone {
    pub(super) fn new(link: Link, id: DeviceId) -> Self {
        Self { link, id }
    }
}

impl Device for Iphone {
    // No view other than the screen has an iPhone implementation.
    fn views(&self) -> Signal<Views> {
        Signal::stored(Views::empty())
    }
}

impl Camera for Iphone {}

#[async_trait(?Send)]
impl Logs for Iphone {}

#[async_trait(?Send)]
impl Shell for Iphone {}

#[async_trait(?Send)]
impl crate::views::files::Files for Iphone {}

#[async_trait(?Send)]
impl crate::views::media::Media for Iphone {}

#[async_trait(?Send)]
impl Screen for Iphone {
    fn features(&self) -> screen::Features {
        screen::Features::STREAMS | screen::Features::PHONE_BUTTONS
    }

    async fn start(&self) -> Result<Session, String> {
        let input = input(self.link, self.id.clone());
        let tracked = tracked(self.link, &self.id).await?;

        Ok(Session {
            stream: tracked.stream,
            input,
            told: tracked.told,
            kept: Box::new((tracked.kept, tracked.lane)),
        })
    }
}

#[async_trait(?Send)]
impl Apps for Iphone {}

#[async_trait(?Send)]
impl Crashes for Iphone {}

#[async_trait(?Send)]
impl Data for Iphone {}

#[async_trait(?Send)]
impl Tweaks for Iphone {}

#[async_trait(?Send)]
impl Location for Iphone {}

#[async_trait(?Send)]
impl Clipboard for Iphone {}

/// A phone that refuses input, such as one without Developer Mode, still
/// shows its screen.
fn input(link: Link, id: DeviceId) -> mpsc::UnboundedSender<Input> {
    let (sender, mut inputs) = mpsc::unbounded::<Input>();

    spawn_local(async move {
        let opened = match link.ios_open(id.clone(), ios::input::TUNNEL).await {
            Ok(tunnel) => ios::input::Input::open(Box::new(tunnel))
                .await
                .map_err(|error| format!("{error:#}")),
            Err(error) => Err(error),
        };
        let Ok(mut input) = opened else {
            return;
        };
        // Without SpringBoard's orientation service every touch is mapped as
        // if the screen were upright.
        let orientation = link
            .ios_open(id.clone(), Orientation::SERVICE)
            .await
            .ok()
            .map(|stream| Orientation::open(Box::new(stream)));
        let mut touching = Touching {
            finger: None,
            turned: InterfaceOrientation::Portrait,
            orientation,
        };

        while let Some(sent) = inputs.next().await {
            if let Err(error) = to_iphone(&mut input, &mut touching, sent).await {
                link.did("input", Some(&id), Err(error));

                break;
            }
        }
    });

    sender
}

fn iphone_button(button: Button) -> Option<ios::input::Button> {
    match button {
        Button::Home => Some(ios::input::Button::Home),
        Button::Power => Some(ios::input::Button::Power),
        Button::VolumeUp => Some(ios::input::Button::VolumeUp),
        Button::VolumeDown => Some(ios::input::Button::VolumeDown),
        Button::Back | Button::Recents => None,
    }
}

struct Touching {
    finger: Option<u32>,
    turned: InterfaceOrientation,
    orientation: Option<Orientation>,
}

/// Maps a point (0 to 1) on the screen as shown to the upright screen the
/// phone's digitizer uses. Landscape right has the top of the phone to the
/// left, as UIKit's `UIInterfaceOrientationLandscapeRight` does.
fn upright(turned: InterfaceOrientation, x: f64, y: f64) -> (f64, f64) {
    match turned {
        InterfaceOrientation::LandscapeRight => (1.0 - y, x),
        InterfaceOrientation::LandscapeLeft => (y, 1.0 - x),
        InterfaceOrientation::PortraitUpsideDown => (1.0 - x, 1.0 - y),
        InterfaceOrientation::Portrait | InterfaceOrientation::Unknown => (x, y),
    }
}

async fn to_iphone(
    input: &mut ios::input::Input,
    touching: &mut Touching,
    sent: Input,
) -> Result<(), String> {
    use ios::input::Phase;

    let sent = match sent {
        Input::Touch {
            action,
            pointer,
            x,
            y,
            width,
            height,
        } => {
            match (action, touching.finger) {
                (TouchAction::Down, None) => touching.finger = Some(pointer),
                (_, Some(down)) if down == pointer => {}
                _ => return Ok(()),
            }

            if matches!(action, TouchAction::Up | TouchAction::Cancel) {
                touching.finger = None;
            }

            // The capture turns with the interface. Only SpringBoard knows which
            // way round a landscape capture is, so it is asked as each touch
            // starts. A portrait capture is taken as upright.
            if action == TouchAction::Down {
                touching.turned = match &mut touching.orientation {
                    Some(orientation) if width > height => orientation
                        .get()
                        .await
                        .unwrap_or(InterfaceOrientation::Portrait),
                    _ => InterfaceOrientation::Portrait,
                };
            }

            let phase = match action {
                TouchAction::Down => Phase::Down,
                TouchAction::Move => Phase::Move,
                TouchAction::Up | TouchAction::Cancel => Phase::Up,
            };
            let (across, down) = upright(
                touching.turned,
                f64::from(x) / f64::from(width.max(1)),
                f64::from(y) / f64::from(height.max(1)),
            );

            input.touch(phase, across, down).await
        }
        Input::Key { down, code, .. } => input.key(&code, down).await,
        Input::Button(button) => {
            let Some(button) = iphone_button(button) else {
                return Ok(());
            };

            match input.button(button, true).await {
                Ok(()) => input.button(button, false).await,
                failed => failed,
            }
        }
        Input::Hold {
            button,
            down,
            repeat,
        } => match iphone_button(button) {
            Some(button) if repeat == 0 => input.button(button, down).await,
            _ => Ok(()),
        },
        _ => Ok(()),
    };

    sent.map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_point_goes_to_the_upright_screen() {
        // The shown top left corner is the phone's top right when its top is
        // to the left, and its bottom left when its top is to the right.
        assert_eq!(
            upright(InterfaceOrientation::LandscapeRight, 0.0, 0.0),
            (1.0, 0.0)
        );
        assert_eq!(
            upright(InterfaceOrientation::LandscapeLeft, 0.0, 0.0),
            (0.0, 1.0)
        );
        assert_eq!(
            upright(InterfaceOrientation::Portrait, 0.25, 0.75),
            (0.25, 0.75)
        );
    }
}
