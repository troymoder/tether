use leptos::prelude::*;
use stylance::classes;

use super::style;
use crate::app::{Global, Theme};
use crate::cache;
use crate::ui;
use crate::ui::action::use_action;
use crate::ui::button::{Button, Tone};
use crate::ui::dialog::Dialog;
use crate::ui::load::{use_load, use_load_local};
use crate::ui::loadable::{Status, Wait};
use crate::ui::pane::Pane;
use crate::ui::segmented::Segmented;
use crate::ui::tabs::Tabs;
use crate::util::size;

#[derive(Clone, Copy, PartialEq)]
enum With {
    AppleId,
    Key,
}

#[component]
fn Signing(open: RwSignal<bool>) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let with = RwSignal::new(With::AppleId);

    view! {
        <Dialog title="iOS signing" class=style::form on_close=move |()| open.set(false)>
            <Tabs
                tabs=vec![(With::AppleId, "Apple ID"), (With::Key, "App Store Connect key")]
                open=with
                on_open=move |to| with.set(to)
            />
            <form on:submit=move |event| {
                event.prevent_default();
                link.unimplemented("iOS signing");
            }>
                {move || match with.get() {
                    With::Key => {
                        view! {
                            <span class=ui::style::hint>
                                "From App Store Connect › Users and Access › Integrations."
                            </span>
                            <label class=ui::style::field>
                                "Issuer ID"
                                <input class=style::mono autocomplete="off" spellcheck="false" />
                            </label>
                            <label class=ui::style::field>
                                "Key ID"
                                <input class=style::mono autocomplete="off" spellcheck="false" />
                            </label>
                            <div class=style::file>
                                <Button on:click=move |_| {
                                    link.unimplemented("iOS signing")
                                }>"Choose the .p8 file"</Button>
                            </div>
                            <div class=style::foot>
                                <Button on:click=move |_| open.set(false)>"Cancel"</Button>
                                <Button submit=true tone=Tone::Blue>
                                    "Save"
                                </Button>
                            </div>
                        }
                            .into_any()
                    }
                    With::AppleId => {
                        view! {
                            <span class=ui::style::hint>
                                "Apps signed with a free account run for 7 days."
                            </span>
                            <label class=ui::style::field>
                                "Apple ID" <input type="email" autocomplete="username" />
                            </label>
                            <label class=ui::style::field>
                                "Password" <input type="password" autocomplete="current-password" />
                            </label>
                            <div class=style::foot>
                                <Button on:click=move |_| open.set(false)>"Cancel"</Button>
                                <Button submit=true tone=Tone::Blue>
                                    "Sign in"
                                </Button>
                            </div>
                        }
                            .into_any()
                    }
                }}
            </form>
        </Dialog>
    }
}

#[allow(unreachable_pub)]
#[component]
fn UsbRow(name: String, id: String, asked: Trigger) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let serial = id.clone();
    let (forgetting, forget) = use_action(None, move |()| {
        let serial = serial.clone();

        async move {
            let allowed = link.allowed_usb().await;

            if let Some((_, _, device)) = allowed.into_iter().find(|(_, id, _)| *id == serial) {
                link.forget_usb(device).await;
            }

            asked.notify();

            Ok(None::<String>)
        }
    });

    view! {
        <div class=style::usb>
            <span>
                <span>{name}</span>
                <code>{id}</code>
            </span>
            <Button quiet=true busy=forgetting on:click=move |_| forget.run(())>
                "Forget"
            </Button>
        </div>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Settings() -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let signing = RwSignal::new(false);
    let asked = Trigger::new();
    // The browser keeps the WebUSB permission itself, so the phones are asked
    // for again whenever one comes or goes and after one is forgotten.
    let allowed = use_load_local(
        move || {
            link.usb.track();
            asked.track();
        },
        move |()| async move { Ok(link.allowed_usb().await) },
    );
    let cleared = Trigger::new();
    let stats = use_load(
        move || cleared.track(),
        |()| async { Ok::<_, String>(cache::stats().await) },
    );
    let (clearing, clear) = use_action(None, move |()| async move {
        cache::clear().await;
        cleared.notify();

        Ok(None::<String>)
    });
    let cached = move || {
        stats.get().ready().map(|stats| {
            let rate = stats.hit_rate().map_or_else(
                || "no reads yet".to_owned(),
                |rate| format!("{:.0}% hits", rate * 100.0),
            );

            format!(
                "{} entries · {} of {} · {rate}",
                stats.entries,
                size(stats.size),
                size(stats.limit)
            )
        })
    };

    view! {
        <Pane title="Settings" class=style::general>
            <div class=ui::style::pane_body>
                <div class=classes!(ui::style::group, style::lines)>
                    <div>
                        <span>"Theme"</span>
                        <Segmented
                            class=style::theme
                            label="Theme"
                            choices=Theme::ALL.map(|one| (one, one.name())).to_vec()
                            chosen=ui.theme
                            on_choose=move |one| ui.recolour(one)
                        />
                    </div>
                </div>
                <div class=classes!(ui::style::group, style::lines)>
                    <div>
                        <span>
                            <span>"iOS signing"</span>
                            <span class=ui::style::hint>
                                "Not set up. Needed to install on iPhones."
                            </span>
                        </span>
                        <Button class=style::set_up on:click=move |_| signing.set(true)>
                            "Set up"
                        </Button>
                    </div>
                </div>
                <div class=classes!(ui::style::group, style::lines)>
                    <div>
                        <span>
                            <span>"Cache"</span>
                            <span class=ui::style::hint>{cached}</span>
                        </span>
                        <Button busy=clearing on:click=move |_| clear.run(())>
                            "Clear cache"
                        </Button>
                    </div>
                </div>
                <h2>"USB devices"</h2>
                <div class=classes!(ui::style::group, style::lines)>
                    <Status of=allowed loading=Wait::Blank empty=(Vec::is_empty, "None.") let:found>
                        {
                            // Keyed, since a row reused for another phone would
                            // keep that phone's Forget. `For` takes only `Send`
                            // rows, so each row finds its UsbDevice again.
                            let found: Vec<(String, String)> = found
                                .into_iter()
                                .map(|(name, id, _)| (name, id))
                                .collect();

                            view! {
                                <For each=move || found.clone() key=Clone::clone let:phone>
                                    <UsbRow name=phone.0 id=phone.1 asked />
                                </For>
                            }
                        }
                    </Status>
                </div>
            </div>
            <Show when=move || signing.get()>
                <Signing open=signing />
            </Show>
        </Pane>
    }
}
