use leptos::prelude::*;
use protocol::{Condition, Platform, Status};
use stylance::classes;

use super::icons;
use super::style;
use super::{Global, Page, Panel};
use crate::link::Linked;
use crate::ui::button::{IconButton, Tone};
use crate::ui::picker::Picker;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Top(full: Memo<bool>, menu_over: RwSignal<bool>) -> impl IntoView {
    let Global { link, router, ui } = expect_context::<Global>();

    let device = Memo::new(move |_| {
        let picked = router.device()?;

        link.device(&picked)
    });

    let runs = move || {
        let device = device.get()?;
        let described = device.described()?;

        Some(match device.id.platform() {
            Platform::Ios => format!("iOS {}", described.release),
            Platform::Android => format!(
                "Android {} · {}",
                described.release,
                described.abis.join(", ")
            ),
        })
    };

    let shown = Memo::new(move |_| {
        let Some(device) = device.get() else {
            return (icons::SQUARE, "No device".to_owned(), Tone::Plain, true);
        };

        let name = &device.name;

        if let Some(run) = &device.emulator {
            return match run.status {
                Status::Shutdown => (icons::PLAY, format!("Start {name}"), Tone::Green, false),
                Status::Booted => (icons::SQUARE, format!("Stop {name}"), Tone::Red, false),
                Status::Booting | Status::Stopping => {
                    (icons::SQUARE, format!("{name} is busy"), Tone::Plain, true)
                }
            };
        }

        if device.condition == Condition::Unavailable {
            return (
                icons::CLOSE,
                format!("{name} is unavailable"),
                Tone::Plain,
                true,
            );
        }

        if device.connected().is_some() {
            (icons::CLOSE, format!("Disconnect {name}"), Tone::Red, false)
        } else if device.routes.first().is_some_and(|route| route.pairing) {
            (icons::PLAY, format!("Pair with {name}"), Tone::Green, false)
        } else {
            (
                icons::PLAY,
                format!("Connect to {name}"),
                Tone::Green,
                false,
            )
        }
    });

    // One button whose handler reads the picked device when pressed. A
    // handler captured per render stays bound to the first device, since
    // leptos rebuilds the button in place and keeps its first `on:click`.
    let press = move |_| {
        let Some(device) = device.get_untracked() else {
            return;
        };

        if let Some(run) = device.emulator {
            match run.status {
                Status::Shutdown => link.boot(device.id),
                Status::Booted => link.shut_down(device.id),
                Status::Booting | Status::Stopping => {}
            }
        } else if device.connected().is_some() {
            link.disconnect(device.id);
        } else {
            link.connect_to(ui, device.id, None);
        }
    };

    view! {
        <header class=style::top>
            <div>
                {move || {
                    let (label, on) = link.linked.with(|linked| match linked {
                        Linked::Connected { interrupted: true, .. } => {
                            ("Connection to the primary interrupted, trying to recover. Open Network.", false)
                        }
                        Linked::Connected { .. } => ("Connected to the primary. Open Network.", true),
                        Linked::Connecting { .. } | Linked::Gate(_) => {
                            ("Not connected to the primary. Open Network.", false)
                        }
                    });

                    view! {
                        <button
                            type="button"
                            class=classes!(style::light, on.then_some(style::on))
                            title=label
                            aria-label=label
                            on:click=move |_| {
                                menu_over.set(false);
                                router.go(Page::Network);
                            }
                        >
                            <span aria-hidden="true"></span>
                        </button>
                    }
                }}
                <button
                    type="button"
                    class=style::brand
                    title="Devices"
                    aria-label="tether: go to Devices"
                    on:click=move |_| {
                        menu_over.set(false);
                        router.go(Page::devices());
                    }
                >
                    "tether"
                </button>
            </div>
            <div>
                <span class=style::runs>{runs}</span>
                <Picker />
                <IconButton
                    paths=Signal::derive(move || shown.with(|shown| shown.0))
                    label=Signal::derive(move || shown.with(|shown| shown.1.clone()))
                    tone=Signal::derive(move || shown.with(|shown| shown.2))
                    disabled=Signal::derive(move || shown.with(|shown| shown.3))
                    busy=Signal::derive(move || {
                        device.with(|device| device.as_ref().is_some_and(|device| link.busy(&device.id)))
                    })
                    class=style::action
                    on:click=press
                />
                <IconButton
                    paths=Signal::derive(move || if full.get() { icons::DEVICE_OFF } else { icons::DEVICE })
                    label=Signal::derive(move || {
                        if full.get() { "Show the device" } else { "Hide the device" }.to_owned()
                    })
                    framed=true
                    lit=full
                    class=style::side
                    attr:aria-pressed=move || full.get().to_string()
                    on:click=move |_| {
                        ui.panel.set(if full.get_untracked() { Panel::Auto } else { Panel::Full })
                    }
                />
            </div>
        </header>
    }
}
