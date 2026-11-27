use leptos::html;
use leptos::prelude::*;

use crate::ui;
use crate::views::devices::style;

fn focus_on_load(field: NodeRef<html::Input>) {
    field.on_load(|field| {
        // It is not in the page yet on load, and only something in the page
        // takes the focus.
        request_animation_frame(move || {
            let _ = field.focus();
        });
    });
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Place(
    host: RwSignal<String>,
    port: RwSignal<String>,
    port_name: &'static str,
    placeholder: &'static str,
    #[prop(optional)] fixed: bool,
    #[prop(optional)] focused: bool,
) -> impl IntoView {
    let port_box = NodeRef::<html::Input>::new();

    if focused {
        focus_on_load(port_box);
    }

    view! {
        <div class=style::place>
            <label class=ui::style::field>
                "IP address"
                <input
                    autocomplete="off"
                    spellcheck="false"
                    placeholder="192.168.1.31"
                    readonly=fixed
                    bind:value=host
                />
            </label>
            <label class=ui::style::field>
                {port_name}
                <input
                    node_ref=port_box
                    inputmode="numeric"
                    autocomplete="off"
                    placeholder=placeholder
                    readonly=fixed
                    bind:value=port
                />
            </label>
        </div>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn CodeInput(code: RwSignal<String>, #[prop(optional)] focused: bool) -> impl IntoView {
    let code_box = NodeRef::<html::Input>::new();

    if focused {
        focus_on_load(code_box);
    }

    view! {
        <span class=style::code>
            {(0..6)
                .map(|place| {
                    view! {
                        <span
                            class=(style::at, move || code.with(|code| code.len().min(5) == place))
                            aria-hidden="true"
                        >
                            {move || code.with(|code| code.chars().nth(place).map(String::from))}
                        </span>
                    }
                })
                .collect_view()}
            <input
                node_ref=code_box
                aria-label="Pairing code, 6 digits"
                inputmode="numeric"
                autocomplete="one-time-code"
                maxlength="6"
                prop:value=move || code.get()
                on:input=move |event| {
                    let digits: String = event_target_value(&event)
                        .chars()
                        .filter(char::is_ascii_digit)
                        .take(6)
                        .collect();

                    code.set(digits);
                }
            />
        </span>
    }
}
