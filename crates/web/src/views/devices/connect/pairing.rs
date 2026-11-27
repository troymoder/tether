use leptos::prelude::*;
use protocol::Request;

use super::address::Address;
use super::fields::{CodeInput, Place};
use super::{Phone, Step, tetherd};
use crate::app::Global;
use crate::ui;
use crate::ui::action::use_action;
use crate::ui::button::{Button, Tone};
use crate::ui::tag::Tag;
use crate::views::devices::scan::Scan;
use crate::views::devices::style;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Pairing(phone: Phone, step: RwSignal<Step>) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let (at, pairing_port) = match &phone {
        Phone::Found { route, .. } => {
            let (host, port) = Address::listed(&route.address).fields();

            (Some(host), port)
        }
        Phone::Typed { host } => (host.clone(), String::new()),
    };
    // A found phone is paired through its route, so its address is only
    // shown.
    let fixed = matches!(phone, Phone::Found { .. });
    let addressed = at.is_some();
    let host = RwSignal::new(at.unwrap_or_default());
    let port = RwSignal::new(pairing_port);
    let code = RwSignal::new(String::new());
    let phone = StoredValue::new(phone);
    let request = move || {
        let code = code.get();

        if code.len() != 6 {
            return None;
        }

        Some(match phone.get_value() {
            Phone::Found { device, route } => Request::Connect {
                device,
                way: Some(route.way),
                code: Some(code),
                take: false,
            },
            Phone::Typed { .. } => Request::Reach {
                address: Address::typed(&host.get(), &port.get())?.to_string(),
                code: Some(code),
            },
        })
    };
    let (busy, pair) = use_action(None, move |request: Request| async move {
        let said = link.call(request).await?;

        // The popup may have closed meanwhile.
        if let Some(host) = host.try_get_untracked() {
            step.try_set(Step::Paired { host });
        }

        Ok(said)
    });
    let unready = Signal::derive(move || request().is_none());

    let form = view! {
        <form on:submit=move |event| {
            event.prevent_default();

            if let Some(request) = untrack(request) {
                pair.run(request);
            }
        }>
            <Place host port port_name="Port" placeholder="37119" fixed />
            <label class=ui::style::field>
                "Pairing code"
                <CodeInput code focused=addressed />
            </label>
            <div class=style::foot>
                <Button
                    class=style::back
                    on:click=move |_| step.set(Step::Start { host: host.get_untracked() })
                >
                    "Back"
                </Button>
                <Button submit=true tone=Tone::Blue busy disabled=unready>"Pair"</Button>
            </div>
        </form>
    };

    view! {
        <h3>"Pair an Android phone"</h3>
        <Tag class=style::via>{format!("From tetherd {}", tetherd())}</Tag>
        <span class=style::on_phone>
            "On the phone: " <span>"Settings › Developer options ›"</span> " Wireless debugging"
        </span>
        // A phone found in pairing mode already shows its code, so it is only
        // typed in. A new one can scan the QR code instead.
        {if fixed {
            form.into_any()
        } else {
            view! {
                <div class=style::ways>
                    <div>
                        <span>"Pair device with QR code"</span>
                        <Scan />
                    </div>
                    <div>
                        <span>"Pair device with pairing code"</span>
                        {form}
                    </div>
                </div>
            }
                .into_any()
        }}
    }
}
