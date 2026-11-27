//! Each kind of device implements the trait each view defines in its
//! `device.rs`, so a view never tells an iPhone from an Android device itself.

use std::rc::Rc;

use async_trait::async_trait;
use leptos::prelude::*;
use protocol::{DeviceId, Platform, Request};

use crate::link::Link;
use crate::views::apps::Apps;
use crate::views::camera::Camera;
use crate::views::clipboard::Clipboard;
use crate::views::crashes::Crashes;
use crate::views::data::Data;
use crate::views::files::Files;
use crate::views::location::Location;
use crate::views::logs::{self, Logs};
use crate::views::media::Media;
use crate::views::preferences::Tweaks;
use crate::views::screen::{self, Screen};
use crate::views::shell::Shell;

mod android;
mod iphone;
mod player;
mod speaker;
mod tracked;

pub(crate) use android::helper::Helper;

bitflags::bitflags! {
    /// Views without a flag are always open.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Views: u16 {
        const LOCATION = 1 << 0;
        const FILES = 1 << 1;
        const MEDIA = 1 << 2;
        const CAMERA = 1 << 3;
        const DATABASE = 1 << 4;
        const CLIPBOARD = 1 << 5;
        const CRASHES = 1 << 6;
        const LOGS = 1 << 7;
        const SHELL = 1 << 8;
        const APPS = 1 << 9;
        const PREFERENCES = 1 << 10;
    }
}

pub(crate) trait Device:
    Screen
    + Apps
    + Camera
    + Clipboard
    + Crashes
    + Data
    + Files
    + Location
    + Logs
    + Media
    + Shell
    + Tweaks
{
    fn views(&self) -> Signal<Views>;
}

pub(crate) fn unsupported(what: &str) -> String {
    format!("unsupported: {what}")
}

#[derive(Clone)]
struct Resting {
    link: Link,
    id: DeviceId,
    emulator: bool,
}

impl Device for Resting {
    fn views(&self) -> Signal<Views> {
        let views = if self.emulator {
            Views::LOGS
        } else {
            Views::empty()
        };

        Signal::stored(views)
    }
}

impl Camera for Resting {}

#[async_trait(?Send)]
impl Screen for Resting {
    /// The buttons stay around the dark screen so the frame keeps the shape it
    /// has once the device is ready.
    fn features(&self) -> screen::Features {
        if self.emulator {
            screen::Features::PHONE_BUTTONS | screen::Features::BACK | screen::Features::RECENTS
        } else {
            screen::Features::PHONE_BUTTONS
        }
    }
}

#[async_trait(?Send)]
impl Apps for Resting {}

#[async_trait(?Send)]
impl Crashes for Resting {}

#[async_trait(?Send)]
impl Data for Resting {}

#[async_trait(?Send)]
impl Tweaks for Resting {}

#[async_trait(?Send)]
impl Location for Resting {}

#[async_trait(?Send)]
impl Clipboard for Resting {}

#[async_trait(?Send)]
impl Logs for Resting {
    fn sources(&self) -> logs::Sources {
        if self.emulator {
            logs::Sources::OUTPUT
        } else {
            logs::Sources::empty()
        }
    }

    async fn output(&self) -> Result<String, String> {
        emulator_output(self.link, &self.id).await
    }
}

#[async_trait(?Send)]
impl Shell for Resting {}

async fn emulator_output(link: Link, id: &DeviceId) -> Result<String, String> {
    let Some(name) = untrack(|| link.run(id)).map(|run| run.avd) else {
        return Err(unsupported("emulator output"));
    };

    link.call(Request::EmulatorLog { name }).await
}

#[async_trait(?Send)]
impl Files for Resting {}

#[async_trait(?Send)]
impl Media for Resting {}

pub(crate) fn of(link: Link, id: DeviceId) -> Rc<dyn Device> {
    let ready = link.device(&id).is_some_and(|device| device.ready());

    match id.platform() {
        Platform::Ios if ready => Rc::new(iphone::Iphone::new(link, id)),
        Platform::Android if ready => Rc::new(android::Android::new(link, id)),
        _ => Rc::new(Resting {
            link,
            emulator: id.is_emulator(),
            id,
        }),
    }
}
