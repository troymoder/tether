use leptos::prelude::*;
use protocol::Webcam;
use stylance::classes;

use super::browser::{Browsing, RemoteFiles};
use super::fed::Cam;
use super::source::{self, Source};
use super::style;
use crate::app::Picked;
use crate::link::capture::{self, Capture, Remote, Remoting, Wanted};
use crate::ui::button::Button;
use crate::ui::chips::{Chip, Chips};
use crate::ui::dropdown::{Choice, Dropdown};
use crate::ui::file_pick::FilePicker;
use crate::ui::load::use_load;
use crate::ui::loadable::Loadable;

fn size_name(width: impl std::fmt::Display, height: impl std::fmt::Display) -> String {
    format!("{width} × {height}")
}

/// The browser gives a camera no name until the page was let use one, so
/// those are numbered.
fn camera_choices(found: Vec<(String, String)>) -> Vec<Choice<String>> {
    found
        .into_iter()
        .filter(|(_, label)| !label.starts_with("dm-"))
        .enumerate()
        .map(|(at, (id, label))| {
            let label = if label.is_empty() {
                format!("Camera {}", at + 1)
            } else {
                label
            };

            Choice::new(id, label)
        })
        .collect()
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn SourceSettings(
    cam: Cam,
    tab: Memo<Source>,
    webcams: Signal<Loadable<Vec<Webcam>>>,
    again: Trigger,
    picker: FilePicker,
    browsing: Browsing,
    open: RwSignal<bool>,
    in_use: Callback<Source, Option<String>>,
    host: StoredValue<String>,
) -> impl IntoView {
    move || match tab.get() {
        Source::Off => {
            view! { <span class=style::said>"The device's camera shows nothing."</span> }.into_any()
        }
        Source::Camera => view! { <BrowserCamera cam /> }.into_any(),
        Source::Webcam => view! { <TetherdWebcam cam webcams again host /> }.into_any(),
        Source::File | Source::Screen => view! { <Chosen cam tab picker in_use /> }.into_any(),
        Source::RemoteFile => view! { <RemoteFiles cam browsing open /> }.into_any(),
    }
}

#[component]
fn BrowserCamera(cam: Cam) -> impl IntoView {
    let feed = expect_context::<Picked>().feed;
    let captured = Memo::new(move |_| {
        let wanted = feed.capturing().filter(|wanted| wanted.lens == cam.lens)?;

        match wanted.what {
            Capture::Camera { id, size } => Some((id, size)),
            Capture::Screen => None,
        }
    });
    // The browser only names its cameras once one of them has been used, so
    // they are asked for again as the capture changes.
    let cameras = use_load(
        move || captured.get(),
        move |_| async move { Ok::<_, String>(camera_choices(capture::cameras().await)) },
    );
    let start = move |id: Option<String>, size: (u32, u32)| {
        let wanted = Wanted {
            lens: cam.lens,
            what: Capture::Camera { id, size },
        };

        feed.capture(wanted);
    };

    move || match captured.get() {
        None => view! {
            <div class=style::picked>
                <span>
                    <span>"Webcam"</span>
                    <span>"This computer's webcam is off"</span>
                </span>
                <Button small=true on:click=move |_| start(None, capture::SIZES[1])>
                    "Start it"
                </Button>
            </div>
        }
        .into_any(),
        Some((id, size)) => {
            let sizes = capture::SIZES
                .into_iter()
                .map(|size| Chip::new(size, size_name(size.0, size.1)))
                .collect::<Vec<_>>();
            let by_size = id.clone();

            view! {
                <span class=style::heading>"Webcam on this computer"</span>
                <div class=style::field>
                    <Dropdown
                        label="Webcam on this computer"
                        placeholder="Any camera"
                        choices=Signal::derive(move || cameras.get().ready().unwrap_or_default())
                        disabled=Signal::derive(move || {
                            cameras.with(|cameras| !matches!(cameras, Loadable::Ready(_)))
                        })
                        chosen=id
                        on_choose=move |id: String| start(Some(id), size)
                    />
                </div>
                <span class=style::rule aria-hidden="true"></span>
                <Chips
                    heading="Size asked of it"
                    chips=sizes
                    chosen=Some(size)
                    on_choose=move |size: (u32, u32)| start(by_size.clone(), size)
                />
            }
            .into_any()
        }
    }
}

#[component]
fn TetherdWebcam(
    cam: Cam,
    webcams: Signal<Loadable<Vec<Webcam>>>,
    again: Trigger,
    host: StoredValue<String>,
) -> impl IntoView {
    let feed = expect_context::<Picked>().feed;
    let used = Memo::new(move |_| {
        let remote = feed.remoting().filter(|remote| remote.lens == cam.lens)?;

        match remote.what {
            Remote::Webcam { path, size, fps } => Some((path, size, fps)),
            Remote::File { .. } => None,
        }
    });
    let choose = move |path: String, size: Option<(u16, u16)>, rate: Option<u16>| {
        let fitting = webcams.with_untracked(|webcams| {
            let Loadable::Ready(found) = webcams else {
                return None;
            };

            let webcam = found.iter().find(|webcam| webcam.path == path)?;

            source::fitting(webcam, size, rate, &cam.settings)
        });

        if let Some((size, fps)) = fitting {
            let wanted = Remoting {
                lens: cam.lens,
                what: Remote::Webcam { path, size, fps },
            };

            feed.start_remote(wanted);
        }
    };
    let refresh = move || view! { <Button on:click=move |_| again.notify()>"Refresh"</Button> };

    move || {
        let found = match webcams.get() {
            Loadable::Loading => {
                return view! { <span class=style::said>"Looking for webcams…"</span> }.into_any();
            }
            Loadable::Failed(why) => {
                return view! {
                    <span class=classes!(style::said, style::bad)>{why}</span>
                    {refresh()}
                }
                .into_any();
            }
            Loadable::Ready(found) if found.is_empty() => {
                return view! {
                    <span class=style::said>{format!("No webcam on tetherd {}", host.get_value())}</span>
                    {refresh()}
                }
                .into_any();
            }
            Loadable::Ready(found) => found,
        };
        let used = used.get();
        let choices: Vec<Choice<String>> = found
            .iter()
            .map(|webcam| {
                let choice = Choice::new(webcam.path.clone(), webcam.name.clone());

                match webcam.sizes.first() {
                    Some(largest) => choice.noted(size_name(largest.width, largest.height)),
                    None => choice,
                }
            })
            .collect();
        let modes = used.clone().map(|(path, size, rate)| {
            let known = found
                .iter()
                .find(|webcam| webcam.path == path)
                .map(|webcam| webcam.sizes.as_slice())
                .unwrap_or_default();
            let sizes = known
                .iter()
                .map(|known| Chip::new((known.width, known.height), size_name(known.width, known.height)))
                .collect::<Vec<_>>();
            let rates = known
                .iter()
                .find(|known| (known.width, known.height) == size)
                .into_iter()
                .flat_map(|known| &known.rates)
                .map(|&fps| Chip::new(fps, format!("{fps} fps")))
                .collect::<Vec<_>>();
            let by_size = path.clone();

            view! {
                <span class=style::rule aria-hidden="true"></span>
                <Chips
                    heading="Size"
                    chips=sizes
                    chosen=Some(size)
                    on_choose=move |size: (u16, u16)| choose(by_size.clone(), Some(size), Some(rate))
                />
                <Chips
                    heading="Frame rate at that size"
                    chips=rates
                    chosen=Some(rate)
                    on_choose=move |rate: u16| choose(path.clone(), Some(size), Some(rate))
                />
            }
        });
        let (size, rate) = match &used {
            Some((_, size, rate)) => (Some(*size), Some(*rate)),
            None => (None, None),
        };

        view! {
            <span class=style::heading>{format!("Webcam on tetherd {}", host.get_value())}</span>
            <div class=style::field>
                <Dropdown
                    label="Webcam on tetherd"
                    placeholder="Choose a webcam"
                    choices=choices
                    chosen=used.map(|(path, ..)| path)
                    on_choose=move |path: String| choose(path, size, rate)
                />
                {refresh()}
            </div>
            {modes}
        }
        .into_any()
    }
}

#[component]
fn Chosen(
    cam: Cam,
    /// `Source::File` or `Source::Screen`. Switching between the two keeps
    /// this mounted, so the button reads it when pressed.
    tab: Memo<Source>,
    picker: FilePicker,
    in_use: Callback<Source, Option<String>>,
) -> impl IntoView {
    let feed = expect_context::<Picked>().feed;
    let screen = move || tab.get() == Source::Screen;
    let again = move || match (screen(), in_use.run(tab.get()).is_some()) {
        (true, false) => "Share a screen or a window",
        (true, true) => "Share another screen or window",
        (false, false) => "Choose an image or a video",
        (false, true) => "Choose another file",
    };

    view! {
        <div class=style::picked>
            <span>
                <span>{move || if screen() { "Sharing" } else { "File" }}</span>
                <span>
                    {move || in_use.run(tab.get()).unwrap_or_else(|| "Nothing yet".to_owned())}
                </span>
            </span>
            <Button
                class=style::again
                on:click=move |_| {
                    if tab.get_untracked() == Source::Screen {
                        let wanted = Wanted {
                            lens: cam.lens,
                            what: Capture::Screen,
                        };

                        feed.capture(wanted);
                    } else {
                        picker.open();
                    }
                }
            >
                {again}
            </Button>
        </div>
    }
}
