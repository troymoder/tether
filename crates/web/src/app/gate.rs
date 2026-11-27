use leptos::prelude::*;
use protocol::Gate;
use stylance::classes;

use crate::app::Global;
use crate::link::Linked;
use crate::ui;
use crate::ui::button;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "gate.module.scss"
);

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Gate() -> impl IntoView {
    let link = expect_context::<Global>().link;
    // A memo, so the form is not built again while the connection's state
    // changes in other ways.
    let gate = Memo::new(move |_| {
        link.linked.with(|linked| match linked {
            Linked::Gate(gate) => Some(gate.clone()),
            _ => None,
        })
    });

    move || match gate.get() {
        Some(Gate::Setup {
            primary,
            you,
            missed,
        }) => view! { <Setup primary you missed /> }.into_any(),
        Some(Gate::Waiting { you }) => view! { <Waiting you /> }.into_any(),
        None => ().into_any(),
    }
}

#[component]
fn Setup(primary: String, you: String, missed: bool) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let code = RwSignal::new(String::new());
    let short = move || code.with(|code| code.len() < 6);
    let claim = move || {
        if !short() {
            link.claim(code.get_untracked());
            code.set(String::new());
        }
    };

    view! {
        <div class=style::gate>
            <form
                class=style::card
                on:submit=move |event| {
                    event.prevent_default();
                    claim();
                }
            >
                <strong class=style::wordmark>"tether"</strong>
                <h1>"Set up tetherd"</h1>
                <span class=ui::style::hint>"Enter the setup code to claim it."</span>
                <dl class=ui::style::kv>
                    <dt>"Its fingerprint"</dt>
                    <dd>{primary}</dd>
                    <dt>"This browser"</dt>
                    <dd>{you}</dd>
                </dl>
                <label class=style::code>
                    "Setup code"
                    <input
                        aria-label="Setup code, six characters"
                        autofocus
                        autocomplete="off"
                        spellcheck="false"
                        maxlength="6"
                        placeholder="······"
                        prop:value=code
                        on:input=move |event| {
                            code.set(
                                event_target_value(&event)
                                    .chars()
                                    .filter(|letter| !letter.is_whitespace())
                                    .take(6)
                                    .collect::<String>()
                                    .to_uppercase(),
                            )
                        }
                    />
                </label>
                <span class=classes!(ui::style::hint, style::line) class=(style::bad, missed)>
                    {if missed { "Wrong code." } else { "Shown in tetherd's terminal." }}
                </span>
                <div class=style::foot>
                    // A plain button element, because `Button` sets
                    // type="button" and would not submit the form.
                    <button
                        class=classes!(button::style::button, button::style::blue)
                        disabled=short
                    >
                        "Claim"
                    </button>
                </div>
            </form>
        </div>
    }
}

#[component]
fn Waiting(you: String) -> impl IntoView {
    view! {
        <div class=style::gate>
            <div class=style::card>
                <strong class=style::wordmark>"tether"</strong>
                <h1>"Waiting for approval"</h1>
                <span class=ui::style::hint>"This browser needs approval."</span>
                <div class=style::print>
                    <span>"This browser's fingerprint"</span>
                    <code>{you}</code>
                </div>
                <span role="status" class=style::waiting>
                    "Waiting…"
                </span>
                <span class=ui::style::hint>"Approve it under Access."</span>
            </div>
        </div>
    }
}
