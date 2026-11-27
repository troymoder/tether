//! The source plays here and its frames are cut in link/capture.rs.

use leptos::prelude::*;
use protocol::{CameraSettings, InstanceId, Lens, VirtualCamera};
use stylance::classes;

use super::Emulator;
use super::holder::HolderRow;
use super::source::Source;
use super::source_box::SourceBox;
use super::stage::Stage;
use super::style;
use crate::app::{Global, Picked};
use crate::link::capture::{self, Feed, Remoting};
use crate::ui;
use crate::ui::pane::Pane;
use crate::ui::segmented::Segmented;
use crate::ui::tag::{Dot, Tag};
use crate::util;

/// An emulator has one camera, on its back or its front.
#[derive(Clone, Copy)]
pub(super) struct Cam {
    pub(super) name: StoredValue<String>,
    pub(super) instance: StoredValue<InstanceId>,
    pub(super) lens: Lens,
    pub(super) settings: CameraSettings,
}

// `Source` is the view's own list of sources, and link/capture.rs knows
// nothing of views, so this method of `Feed` is defined here.
impl Feed {
    fn source(self, lens: Lens) -> Source {
        if let Some(remote) = self.remoting().filter(|remote| remote.lens == lens) {
            return match remote.what {
                capture::Remote::Webcam { .. } => Source::Webcam,
                capture::Remote::File { .. } => Source::RemoteFile,
            };
        }

        if let Some(wanted) = self.capturing().filter(|wanted| wanted.lens == lens) {
            return match wanted.what {
                capture::Capture::Screen => Source::Screen,
                capture::Capture::Camera { .. } => Source::Camera,
            };
        }

        match self.played() {
            Some(played) if played.lens == lens => Source::File,
            _ => Source::Off,
        }
    }
}

/// The cameras an emulator has are part of its settings, since they only
/// change when it starts.
#[allow(unreachable_pub)]
#[component]
pub(super) fn Fed(emulator: Memo<Emulator>) -> impl IntoView {
    let feed = expect_context::<Picked>().feed;
    let camera = Memo::new(move |_| emulator.with(|emulator| emulator.settings.camera));
    let lens = Memo::new(move |_| camera.get().map(|camera| camera.lens));
    // The tabs only show the side. The camera's side is set on the
    // emulator's page, so choosing one here does nothing.
    let on_choose = |_: Option<Lens>| {};
    let greyed = Callback::new(move |one: Option<Lens>| one != lens.get());
    // A file is sent from this browser too, so it is live like the camera.
    let live = move || {
        feed.sent().map(|why| {
            view! {
                <Tag tone=Dot::Bad title=why.to_owned() class=style::live attr:role="status">
                    "Live"
                </Tag>
            }
        })
    };

    view! {
        <Pane
            title="Camera"
            class=style::view
            noted=move || live
            tools=move || {
                view! {
                    <Segmented
                        class=style::lenses
                        label="Which camera of the device"
                        choices=vec![(Some(Lens::Back), "Back"), (Some(Lens::Front), "Front")]
                        chosen=lens
                        on_choose
                        greyed
                    />
                }
            }
        >
            <div class=ui::style::pane_body>
                {move || match camera.get() {
                    Some(VirtualCamera { lens, settings }) => {
                        view! { <FedCamera emulator lens settings /> }.into_any()
                    }
                    None => view! {
                        <div class=classes!(style::stage, style::plain)>
                            <div class=classes!(style::pad, style::blank)>
                                <span>"The camera shows nothing"</span>
                            </div>
                        </div>
                        <div class=style::under>
                            <span>"The camera is off on the emulator's page"</span>
                        </div>
                    }
                    .into_any(),
                }}
            </div>
        </Pane>
    }
}

#[component]
fn FedCamera(emulator: Memo<Emulator>, lens: Lens, settings: CameraSettings) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let feed = expect_context::<Picked>().feed;
    let cam = Cam {
        name: StoredValue::new(emulator.with_untracked(|emulator| emulator.name.clone())),
        instance: StoredValue::new(emulator.with_untracked(|emulator| emulator.instance.clone())),
        lens,
        settings,
    };
    let shown = Memo::new(move |_| feed.source(lens));
    // The feed tetherd runs from its machine by itself, which is previewed
    // here once the page is opened again.
    let running = Memo::new(move |_| {
        emulator.with(|emulator| {
            emulator
                .remote
                .as_ref()
                .filter(|feed| feed.lens == lens)
                .map(Remoting::of)
        })
    });

    Effect::new(move |_| {
        if let Some(running) = running.get()
            && untrack(|| feed.remoting()).as_ref() != Some(&running)
        {
            feed.watch_remote(running);
        }
    });

    let other = Memo::new(move |_| {
        emulator
            .with(|emulator| emulator.fed_by.clone())
            .filter(|feeder| !link.me(&feeder.print))
    });
    // The row only changes kind with this, so it stays mounted when one
    // feeding browser follows another.
    let held = Memo::new(move |_| other.with(Option::is_some));
    let lost = Memo::new(move |_| {
        other.with(|other| {
            other.as_ref().is_some_and(|by| {
                feed.lost_camera
                    .with(|lost| lost.as_ref().is_some_and(|lost| lost.print == by.print))
            })
        })
    });
    let upright = Memo::new(move |_| emulator.with(|emulator| emulator.upright));
    let blank = Signal::derive(move || {
        match (
            other.with(|by| by.as_ref().map(|by| by.name.clone())),
            lost.get(),
        ) {
            (Some(name), true) => format!("Taken over by {name}"),
            (Some(name), false) => format!("Fed by {name}"),
            (None, _) => "The camera shows nothing".to_owned(),
        }
    });
    let about = move || {
        format!(
            "What the {} camera of {} shows · {} × {} · {} fps",
            lens.name(),
            emulator.with(|emulator| emulator.label.clone()),
            settings.width,
            settings.height,
            settings.fps,
        )
    };
    let sending = move || {
        feed.sending().map(|sending| {
            let lossless = if feed.lossless() { "lossless · " } else { "" };

            format!(
                "{} fps · {lossless}{}/frame · {:.0} ms",
                sending.pictures,
                util::size(sending.packed.into()),
                sending.took,
            )
        })
    };

    view! {
        {move || match held.get() {
            true => view! { <HolderRow cam by=other lost /> }.into_any(),
            false => {
                view! {
                    <SourceBox
                        cam
                        shown
                        tetherd_feeds=Signal::derive(move || running.with(Option::is_some))
                    />
                }
                    .into_any()
            }
        }}
        <Stage cam upright shown blank />
        <div class=style::under>
            {move || match other.get() {
                Some(by) => view! { <span>{format!("{} · {}", by.name, by.print)}</span> }.into_any(),
                None => view! {
                    <span>{about}</span>
                    <code>{sending}</code>
                }
                .into_any(),
            }}
        </div>
    }
}
