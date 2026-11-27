use leptos::prelude::*;
use protocol::{DeviceId, InstanceId, Request, Route};
use stylance::classes;

use super::style;
use crate::app::{Adding, Global, Wanted};
use crate::app::{Icon, icons};
use crate::ui;
use crate::ui::action::use_action;
use crate::ui::button::{Button, Tone};
use crate::ui::dropdown::Dropdown;
use crate::ui::loadable::{Status, Wait};
use crate::ui::tag::{Dot, Tag};
use crate::views::settings;

mod address;
mod fields;
mod pairing;

use address::Address;
use fields::Place;
use pairing::Pairing;

#[derive(Clone)]
enum Step {
    Start {
        host: String,
    },
    Pairing(Phone),
    /// Connecting happens at another port than pairing, which is asked for
    /// next.
    Paired {
        host: String,
    },
}

#[derive(Clone)]
enum Phone {
    Found { device: DeviceId, route: Route },
    Typed { host: Option<String> },
}

/// The host the page came from is the tetherd a phone is paired and
/// connected from.
fn tetherd() -> String {
    window().location().host().unwrap_or_default()
}

fn use_connect() -> (Signal<bool>, Callback<Address>) {
    let Global { link, ui, .. } = expect_context::<Global>();

    use_action(None, move |address: Address| {
        let address = address.to_string();
        let request = Request::Reach {
            address: address.clone(),
            code: None,
        };

        async move {
            let said = link.call(request).await?;

            ui.wanted.set(Some(Wanted::Address(address)));
            ui.adding.set(None);

            Ok(said)
        }
    })
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Connect() -> impl IntoView {
    let ui = expect_context::<Global>().ui;
    let step = RwSignal::new(ui.adding.with_untracked(|adding| match adding {
        Some(Adding::Pairing(device, route)) => Step::Pairing(Phone::Found {
            device: device.clone(),
            route: route.clone(),
        }),
        Some(Adding::Any) | None => Step::Start {
            host: String::new(),
        },
    }));

    view! {
        <div class=style::connect>
            {move || match step.get() {
                Step::Start { host } => view! { <Start host step /> }.into_any(),
                Step::Pairing(phone) => view! { <Pairing phone step /> }.into_any(),
                Step::Paired { host } => view! { <Paired host /> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn Start(host: String, step: RwSignal<Step>) -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let host = RwSignal::new(host);
    let port = RwSignal::new(String::new());
    let (busy, connect) = use_connect();
    let address = move || Address::typed(&host.get(), &port.get());
    let unready = Signal::derive(move || address().is_none());

    view! {
        <h3>"USB"</h3>
        <p>"Android or iPhone, plugged into this computer."</p>
        <div class=style::usb>
            <Button
                class=style::choose
                on:click=move |_| {
                    ui.adding.set(None);
                    link.choose_usb();
                }
            >
                <Icon paths=icons::USB />
                "Choose a USB device"
            </Button>
            <code>"WebUSB · Chromium browsers"</code>
        </div>
        <h3 class=style::next>"Android over Wi-Fi"</h3>
        <p>"On the phone: Wireless debugging › IP address & Port."</p>
        <form on:submit=move |event| {
            event.prevent_default();

            if let Some(address) = untrack(address) {
                connect.run(address);
            }
        }>
            <label class=ui::style::field>
                "Connects from"
                <Status of=settings::here() loading=Wait::Blank let:choices>
                    <Dropdown
                        label="Connects from"
                        class=style::from
                        choices
                        chosen=Signal::derive(move || link.primary())
                        // Only the primary connects to phones, so any other
                        // choice has nothing behind it.
                        on_choose=move |from: InstanceId| {
                            if untrack(|| link.primary()) != Some(from) {
                                link.unimplemented("connect from another instance");
                            }
                        }
                    />
                </Status>
            </label>
            <Place host port port_name="Port" placeholder="40213" />
            <div class=style::between>
                <Button on:click=move |_| {
                    let typed = Some(host.get_untracked()).filter(|host| !host.trim().is_empty());

                    step.set(Step::Pairing(Phone::Typed { host: typed }));
                }>"Pair a new phone"</Button>
                <Button submit=true tone=Tone::Blue busy disabled=unready>"Connect"</Button>
            </div>
        </form>
        <div class=classes!(style::between, style::last)>
            <p>"Phones plugged into an instance appear on their own."</p>
            <Button on:click=move |_| ui.adding.set(None)>"Close"</Button>
        </div>
    }
}

#[component]
fn Paired(host: String) -> impl IntoView {
    let ui = expect_context::<Global>().ui;
    let host = RwSignal::new(host);
    let port = RwSignal::new(String::new());
    let (busy, connect) = use_connect();
    let address = move || Address::typed(&host.get(), &port.get());
    let unready = Signal::derive(move || address().is_none());

    view! {
        <Tag tone=Dot::On class=style::paired>
            {move || format!("Paired with {}, from tetherd {}", host.get(), tetherd())}
        </Tag>
        <p>"On the phone: Wireless debugging › IP address & Port."</p>
        <form on:submit=move |event| {
            event.prevent_default();

            if let Some(address) = untrack(address) {
                connect.run(address);
            }
        }>
            <Place host port port_name="Port" placeholder="40213" focused=true />
            <div class=style::foot>
                <Button on:click=move |_| ui.adding.set(None)>"Cancel"</Button>
                <Button submit=true tone=Tone::Blue busy disabled=unready>"Connect"</Button>
            </div>
        </form>
    }
}
