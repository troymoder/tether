use leptos::prelude::*;
use stylance::classes;

use super::figures::rate;
use super::style;
use crate::app::Global;
use crate::link::Linked;

#[allow(unreachable_pub)]
#[component]
pub(super) fn ConnectionRow(
    open: RwSignal<bool>,
    #[prop(into)] graph: Signal<String>,
) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let live = move || link.connected.get();
    // The page has no TURN relay, so a connection that is up is direct.
    let state = move || if live() { "Direct" } else { "Disconnected" };
    let latest = move || {
        link.figures
            .traffic
            .with(|traffic| traffic.back().copied())
            .filter(|_| live())
    };

    view! {
        <button
            type="button"
            class=style::row
            aria-label=move || format!("This browser to {}, {}", link.primary_name(), state())
            aria-expanded=move || open.get().to_string()
            on:click=move |_| open.update(|open| *open = !*open)
        >
            <span class=style::from>"This browser"</span>
            <span class=style::to>
                <span aria-hidden="true">"→ "</span>
                {move || link.primary_name()}
            </span>
            <code class=style::kind>
                {move || {
                    link.linked
                        .with(|linked| match linked {
                            Linked::Connected { route: Some(route), .. } => {
                                format!("WebRTC ({route})")
                            }
                            _ => "WebRTC".to_owned(),
                        })
                }}
            </code>
            <span class=style::state class=(style::off, move || !live())>
                <span aria-hidden="true"></span>
                {state}
            </span>
            <code class=classes!(
                style::down, style::figure
            )>{move || latest().map(|(down, _)| format!("↓ {}", rate(down)))}</code>
            <code class=classes!(
                style::up, style::figure
            )>{move || latest().map(|(_, up)| format!("↑ {}", rate(up)))}</code>
            <span class=style::graph inner_html=move || graph.get()></span>
        </button>
    }
}
