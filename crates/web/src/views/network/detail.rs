use std::collections::VecDeque;

use leptos::prelude::*;
use stylance::classes;
use wasm_bindgen::JsCast;

use super::figures::{lasted, loss, rate, trip};
use super::style;
use crate::app::Global;
use crate::link::{Linked, MINUTE};
use crate::ui;
use crate::util::size;

struct Probe {
    line: String,
    place: String,
    said: String,
}

/// The series holds up to the last `MINUTE` seconds, oldest first, and `at`
/// counts across the whole minute. None for a second before the series began.
fn second<T: Copy>(series: &VecDeque<T>, at: usize) -> Option<T> {
    let first = MINUTE - series.len().min(MINUTE);

    series.get(at.checked_sub(first)?).copied()
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Detail(#[prop(into)] graph: Signal<String>) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let figures = link.figures;
    let probed = RwSignal::new(None::<usize>);
    let read = move || {
        let at = probed.get()?;
        let (down, up) = figures.traffic.with(|traffic| second(traffic, at))?;
        let ago = match MINUTE - 1 - at {
            0 => "now".to_owned(),
            ago => format!("{ago} s ago"),
        };
        let measured = figures
            .round_trips
            .with(|trips| second(trips, at))
            .flatten()
            .map(trip);
        let lost = figures
            .losses
            .with(|losses| second(losses, at))
            .flatten()
            .map(|share| format!("lost {}", loss(share)));
        let said = [
            Some(ago),
            Some(format!("↓ {}", rate(down))),
            Some(format!("↑ {}", rate(up))),
            measured,
            lost,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("  ");
        let across = at as f32 / (MINUTE - 1) as f32 * 100.0;
        // The tooltip goes on whichever side of the cursor has more room.
        let place = if at * 2 > MINUTE {
            format!("right: {:.1}%", 100.0 - across)
        } else {
            format!("left: {across:.1}%")
        };

        Some(Probe {
            line: format!("left: {across:.2}%"),
            place,
            said,
        })
    };

    view! {
        <div class=style::whole>
            <span
                class=classes!(style::graph, style::large)
                on:pointermove=move |event| {
                    let Some(graph) = event
                        .current_target()
                        .and_then(|graph| graph.dyn_into::<web_sys::Element>().ok())
                    else {
                        return;
                    };

                    let within = graph.get_bounding_client_rect();
                    let across = ((event.client_x() - within.left()) / within.width()).clamp(0.0, 1.0);

                    probed.set(Some((across * (MINUTE - 1) as f64).round() as usize));
                }
                on:pointerleave=move |_| probed.set(None)
            >
                <span inner_html=move || graph.get()></span>
                {move || {
                    read()
                        .map(|Probe { line, place, said }| {
                            view! {
                                <span class=style::cursor style=line aria-hidden="true"></span>
                                <span class=style::tip style=place role="status">
                                    {said}
                                </span>
                            }
                        })
                }}
            </span>
            <dl class=ui::style::kv>
                <dt>"Connected for"</dt>
                <dd>
                    {move || {
                        // Tracks the traffic, which ticks every second, so
                        // the duration stays current.
                        figures.traffic.track();

                        link.linked
                            .with(|linked| match linked {
                                Linked::Connected { since, .. } => lasted(js_sys::Date::now() - since),
                                _ => "Not connected".to_owned(),
                            })
                    }}
                </dd>
                <dt>"Round trip"</dt>
                <dd>
                    {move || {
                        figures
                            .round_trips
                            .with(|trips| trips.back().copied().flatten())
                            .map_or_else(|| "Not measured".to_owned(), trip)
                    }}
                </dd>
                <dt>"Packets lost"</dt>
                <dd>
                    // The screen's RTP loss while it streams, otherwise the
                    // share of unanswered ICE checks.
                    {move || {
                        figures
                            .losses
                            .with(|losses| losses.back().copied().flatten())
                            .or_else(|| figures.lost.get())
                            .map_or_else(|| "Not measured".to_owned(), loss)
                    }}
                </dd>
                <dt>"Received"</dt>
                <dd>{move || size(figures.carried.get().0)}</dd>
                <dt>"Sent"</dt>
                <dd>{move || size(figures.carried.get().1)}</dd>
            </dl>
        </div>
    }
}
