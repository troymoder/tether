//! The device's [`Screen`] reports what it can do and starts itself, so
//! nothing here tells kinds of device apart.
//!
//! Every change to `Control::playing` is one of the steps in playing.rs. This
//! file takes the steps and holds what keeps the stream or the pictures
//! coming.

use std::any::Any;
use std::rc::Rc;
use std::time::Duration;

use futures_util::StreamExt;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use protocol::{DeviceId, Media, access};
use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement, ImageData};

use super::playing::{Playing, Wanted};
use super::stats::Counted;
use super::{Control, Features, Screen, Session, Told};
use crate::app::{Global, Panel, Picked};
use crate::device::Device;
use crate::link::Link;
use crate::link::lane::Lane;
use crate::util::sleep;
use crate::views::clipboard::Board;

/// Dropping it ends the screen.
enum Held {
    Pictures {
        _lane: Lane,
    },
    /// `session` is None until the screen has started.
    Stream {
        screen: Rc<dyn Screen>,
        session: Option<Box<dyn Any>>,
    },
}

#[derive(Clone, Copy)]
pub(super) struct Stream {
    link: Link,
    id: StoredValue<DeviceId>,
    device: Signal<Rc<dyn Device>, LocalStorage>,
    control: Control,
    board: Board,
    canvas: NodeRef<html::Canvas>,
    video: NodeRef<html::Video>,
    pub(super) counted: StoredValue<Counted>,
    wanted: Memo<Wanted>,
    /// The current stream's number. A stream that finds it changed is stale.
    streamed: StoredValue<u32>,
    /// The number of the last rotation query, since an earlier one can answer
    /// after it with a stale rotation.
    looked: StoredValue<u32>,
    held: StoredValue<Option<Held>, LocalStorage>,
}

impl Stream {
    pub(super) fn new(canvas: NodeRef<html::Canvas>, video: NodeRef<html::Video>) -> Self {
        let Global { link, ui, .. } = expect_context::<Global>();
        let Picked {
            id,
            device,
            listed,
            board,
            control,
            ..
        } = expect_context::<Picked>();
        let wanted = Memo::new(move |_| {
            // A full-width panel hides the device's pane, and nothing is
            // streamed then.
            let hidden = ui.panel.get() == Panel::Full;
            let ready = Screen::features(&*device.get()).contains(Features::STREAMS);
            let status = listed.with(|listed| listed.emulator.as_ref().map(|run| run.status));

            Wanted::of(hidden, ready, status)
        });

        Stream {
            link,
            id: StoredValue::new(id),
            device,
            control,
            board,
            canvas,
            video,
            counted: StoredValue::new(Counted::new()),
            wanted,
            streamed: StoredValue::new(0),
            looked: StoredValue::new(0),
            held: StoredValue::new_local(None),
        }
    }

    pub(super) fn follow(self) {
        // The effect's value is the last `Wanted`, and true if a change to it
        // has yet to be acted on.
        Effect::new(move |before: Option<(Wanted, bool)>| {
            let wanted = self.wanted.get();
            // A change while the connection is down still counts once it is
            // back.
            let moved = before.is_none_or(|(last, owed)| owed || last != wanted);

            if !self.link.connected.get() {
                return (wanted, moved);
            }

            self.ask(moved);

            (wanted, false)
        });
    }

    pub(super) fn reconnect(self) {
        self.control.playing.try_set(Playing::Idle);
        self.ask(false);
    }

    pub(super) fn turn(self, to: Option<u16>) {
        let Some(screen) = self.screen() else {
            return;
        };

        spawn_local(async move {
            match screen.turn(to).await {
                Ok(()) => self.look(),
                Err(error) => self.link.say(error),
            }
        });
    }

    pub(super) fn stop(self) {
        self.end();
        self.control.playing.try_set(Playing::Idle);
    }

    /// Readers are only notified when the state changed, since this runs for
    /// every frame.
    fn step(self, step: impl FnOnce(Playing) -> Playing) -> Option<Playing> {
        self.control.playing.try_maybe_update(|playing| {
            let next = step(*playing);
            let changed = next != *playing;

            *playing = next;

            (changed, next)
        })
    }

    fn screen(self) -> Option<Rc<dyn Screen>> {
        self.held
            .try_with_value(|held| match held {
                Some(Held::Stream { screen, .. }) => Some(screen.clone()),
                Some(Held::Pictures { .. }) | None => None,
            })
            .flatten()
    }

    fn end(self) {
        self.streamed.try_update_value(|number| *number += 1);
        self.control.input.try_set_value(None);

        if let Some(video) = self.video.get_untracked() {
            video.set_src_object(None);
        }

        self.held.try_set_value(None);
    }

    fn ask(self, moved: bool) {
        if self.link.connected.try_get_untracked() != Some(true) {
            return;
        }

        let Some(wanted) = self.wanted.try_get_untracked() else {
            return;
        };

        self.end();

        match self.step(|playing| playing.asked(wanted, moved)) {
            Some(Playing::Pictures { .. }) => self.pictures(self.id.get_value()),
            Some(Playing::Starting { .. }) => self.start(self.device.get_untracked()),
            _ => {}
        }
    }

    /// Reads the rotation at once and again 1.2 s later. A screen can rotate
    /// from one side to the other without the video changing shape, so the
    /// second read catches that. Queries overlap while it rotates, so only
    /// the last one counts.
    fn look(self) {
        let Some(screen) = self
            .screen()
            .filter(|screen| screen.features().contains(Features::ROTATES))
        else {
            return;
        };

        let number = self.streamed.get_value();
        let mine = self.looked.get_value().wrapping_add(1);

        self.looked.set_value(mine);

        let current = move || {
            self.streamed.try_get_value() == Some(number)
                && self.looked.try_get_value() == Some(mine)
        };

        spawn_local(async move {
            for wait in [0, 1200] {
                sleep(Duration::from_millis(wait)).await;

                if !current() {
                    return;
                }

                let turned = screen.turned().await;

                if let Some(turned) = turned.filter(|_| current()) {
                    self.step(|playing| playing.read(turned));
                }
            }
        });
    }

    fn ended(self, number: u32, failed: Option<String>) {
        if self.streamed.try_get_value() != Some(number) {
            return;
        }

        self.end();

        let Some(now) = self.step(Playing::ended) else {
            return;
        };

        // A failure is retried like a stream that ended. The error is shown
        // once, when the stream is given up on.
        if let Some(error) = failed
            && matches!(now, Playing::GaveUp { .. })
        {
            self.link
                .did("screen", self.id.try_get_value().as_ref(), Err(error));
        }

        let Some(wait) = now.wait() else {
            return;
        };

        // tetherd's state arrives a little after the stream ends. Waiting
        // lets `ask` see a device that went away or an emulator that is
        // stopping, since it reads `wanted` when it runs.
        set_timeout(
            move || {
                // Something else requested the screen meanwhile.
                if self.streamed.try_get_value() != Some(number + 1) {
                    return;
                }

                self.ask(false);
            },
            Duration::from_secs(wait),
        );
    }

    fn start(self, screen: Rc<dyn Screen>) {
        let number = self.streamed.get_value();
        let current = move || self.streamed.try_get_value() == Some(number);

        self.held.set_value(Some(Held::Stream {
            screen: screen.clone(),
            session: None,
        }));

        spawn_local(async move {
            let started = screen.start().await;

            // Returning drops a session that is not wanted any more, which
            // ends it.
            if !current() {
                return;
            }

            let failed = match started {
                Ok(session) => self.play(session, current).await,
                Err(error) => Some(error),
            };

            self.ended(number, failed);
        });
    }

    /// Returns the error if the screen failed.
    async fn play(self, session: Session, current: impl Fn() -> bool) -> Option<String> {
        let Session {
            stream,
            input,
            mut told,
            kept,
        } = session;

        if let Some(video) = self.video.get_untracked() {
            video.set_src_object(Some(&stream));
            // `play` rejects when the stream is replaced right away, which is
            // harmless. The promise is awaited so the rejection is handled.
            if let Ok(started) = video.play() {
                spawn_local(async move {
                    let _ = wasm_bindgen_futures::JsFuture::from(started).await;
                });
            }

            self.counted.try_update_value(Counted::restart);
            frames(self, video);
        }

        self.held.update_value(|held| {
            if let Some(Held::Stream { session, .. }) = held {
                *session = Some(kept);
            }
        });

        self.control.input.set_value(Some(input));
        self.look();

        while current() {
            match told.next().await? {
                // The video's clock starts over with each codec message, which
                // is sent again when the screen changes shape, such as when
                // it rotates.
                Told::Codec(_) => {
                    self.counted.try_update_value(Counted::restart);
                    self.look();
                }
                Told::Clipboard(text) => self.board.told(text),
                Told::Shown { behind } => {
                    self.counted
                        .try_update_value(|counted| counted.shown(behind));
                }
                Told::Received(bytes) => {
                    self.counted
                        .try_update_value(|counted| counted.bytes += bytes);
                }
                Told::Ended(error) => return error,
            }
        }

        None
    }

    fn pictures(self, device: DeviceId) {
        let Some(lane) = self.link.lane(&Media::Watch { device }) else {
            return;
        };

        let mut pictures = lane.heard();
        let number = self.streamed.get_value();

        self.held.set_value(Some(Held::Pictures { _lane: lane }));

        spawn_local(async move {
            while let Some(came) = pictures.next().await
                && self.streamed.try_get_value() == Some(number)
            {
                let Ok(picture) = access::<protocol::Picture>(&came) else {
                    break;
                };

                let width = u32::from(picture.width.to_native());
                let height = u32::from(picture.height.to_native());
                let swelled = protocol::swell(&picture.data, width as usize * height as usize * 4);

                if let (Some(canvas), Some(swelled)) = (self.canvas.get_untracked(), swelled)
                    && paint(&canvas, &swelled, width, height)
                {
                    self.step(|playing| playing.pictured((width, height)));
                }
            }
        });
    }
}

/// `picture` is RGBA. Returns true if it was drawn.
fn paint(canvas: &HtmlCanvasElement, picture: &[u8], width: u32, height: u32) -> bool {
    let Ok(picture) = ImageData::new_with_u8_clamped_array_and_sh(Clamped(picture), width, height)
    else {
        return false;
    };

    if canvas.width() != width || canvas.height() != height {
        canvas.set_width(width);
        canvas.set_height(height);
    }

    let Ok(Some(context)) = canvas.get_context("2d") else {
        return false;
    };

    let context: CanvasRenderingContext2d = context.unchecked_into();

    context.put_image_data(&picture, 0, 0).is_ok()
}

/// Uses `requestVideoFrameCallback`, which web-sys does not have.
fn frames(stream: Stream, video: HtmlVideoElement) {
    let number = stream.streamed.get_value();
    let asking = video.clone();
    let shown = move |_: JsValue, metadata: JsValue| {
        if stream.streamed.try_get_value() != Some(number) {
            return;
        }

        let field = |key: &str| {
            js_sys::Reflect::get(&metadata, &key.into())
                .ok()
                .and_then(|value| value.as_f64())
        };

        // The time from the frame's last packet arriving to its display. A
        // frame decoded in the page has no packets and is counted at decode
        // time instead.
        if let (Some(arrived), Some(shows)) = (field("receiveTime"), field("expectedDisplayTime")) {
            stream
                .counted
                .try_update_value(|counted| counted.shown(shows - arrived));
        }

        // A frame with a size makes the video live.
        if video.video_width() > 0 {
            let size = (video.video_width(), video.video_height());

            stream.step(|playing| playing.framed(size));
        }

        frames(stream, video.clone());
    };
    let shown = wasm_bindgen::closure::Closure::once_into_js(shown);

    if let Ok(request) = js_sys::Reflect::get(&asking, &"requestVideoFrameCallback".into())
        && let Ok(request) = request.dyn_into::<js_sys::Function>()
    {
        let _ = request.call1(&asking, &shown);
    }
}
