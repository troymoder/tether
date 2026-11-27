use leptos::prelude::*;
use leptos::{ev, html};
use stylance::classes;
use wasm_bindgen::JsCast;

use super::style;

/// The share of the bar's length within which a stop snaps.
const SNAP: f32 = 0.012;

pub(super) fn rounded(zoom: f32) -> f32 {
    if zoom < 1.0 {
        (zoom * 100.0).round() / 100.0
    } else if zoom < 10.0 {
        (zoom * 10.0).round() / 10.0
    } else {
        zoom.round()
    }
}

pub(super) fn zoom_text(zoom: f32) -> String {
    let text = format!("{zoom:.2}");

    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn ZoomBar(
    least: f32,
    most: f32,
    zoom: Signal<f32>,
    #[prop(into)] on_zoom: Callback<f32>,
) -> impl IntoView {
    let bar = NodeRef::<html::Div>::new();
    // From 0 to 1 along the bar, on a log scale.
    let along = move |zoom: f32| (zoom / least).ln() / (most / least).ln();
    let at = move |along: f32| rounded(least * (most / least).powf(along.clamp(0.0, 1.0)));
    // The range's ends rounded inwards so they stay in it, and the round
    // zooms people reach for between them.
    let mut stops = vec![
        (least * 10.0).ceil() / 10.0,
        1.0,
        2.0,
        5.0,
        10.0,
        (most * 10.0).floor() / 10.0,
    ];

    stops.retain(|stop| (least..=most).contains(stop));
    stops.dedup();

    let ends = (stops.first().copied(), stops.last().copied());
    let stops = StoredValue::new(stops);
    // A press in progress: where it started, the stop label under it, and
    // true once it has moved.
    let press = StoredValue::new(None::<(f64, Option<f32>, bool)>);

    let snapped = move |to: f32| {
        let near = stops.with_value(|stops| {
            stops
                .iter()
                .copied()
                .find(|stop| (along(*stop) - to).abs() <= SNAP)
        });

        on_zoom.run(near.unwrap_or_else(|| at(to)));
    };

    let slide = move |x: f64| {
        if let Some(bar) = bar.get_untracked() {
            let place = bar.get_bounding_client_rect();

            snapped(((x - place.left()) / place.width()) as f32);
        }
    };
    // The stop whose label is under the pointer, among the labels showing.
    let label_at = move |x: f64| {
        let bar = bar.get_untracked()?;
        let labels = bar.query_selector_all("[data-zoom]").ok()?;

        (0..labels.length())
            .filter_map(|index| labels.get(index)?.dyn_into::<web_sys::HtmlElement>().ok())
            .find(|label| {
                let place = label.get_bounding_client_rect();

                place.width() > 0.0 && (place.left()..=place.right()).contains(&x)
            })
            .and_then(|label| label.dataset().get("zoom")?.parse().ok())
    };

    view! {
        <div class=style::zoom_row>
            <div
                node_ref=bar
                class=style::zoom_bar
                on:pointerdown=move |event: ev::PointerEvent| {
                    if event.button() != 0 {
                        return;
                    }

                    event.prevent_default();

                    if let Some(bar) = bar.get_untracked() {
                        let _ = bar.set_pointer_capture(event.pointer_id());
                    }

                    press.set_value(Some((event.client_x(), label_at(event.client_x()), false)));
                }
                on:pointermove=move |event: ev::PointerEvent| {
                    let Some((from, label, moved)) = press.get_value() else {
                        return;
                    };

                    // A press that moves under 4px is a click on a stop.
                    if !moved && (event.client_x() - from).abs() < 4.0 {
                        return;
                    }

                    press.set_value(Some((from, label, true)));
                    slide(event.client_x());
                }
                on:pointerup=move |event: ev::PointerEvent| {
                    match press.get_value() {
                        Some((_, Some(stop), false)) => on_zoom.run(stop),
                        Some((_, None, false)) => slide(event.client_x()),
                        _ => {}
                    }

                    press.set_value(None);
                }
                on:pointercancel=move |_| press.set_value(None)
            >
                <span class=style::zoom_fill aria-hidden="true" style=move || format!("width: {}%", along(zoom.get()) * 100.0)></span>
                // For arrow keys and screen readers.
                <input
                    type="range"
                    aria-label="Zoom"
                    min="0"
                    max="1000"
                    step="1"
                    prop:value=move || ((along(zoom.get()) * 1000.0).round() as i32).to_string()
                    on:input=move |event| {
                        if let Ok(value) = event_target_value(&event).parse::<f32>() {
                            snapped(value / 1000.0);
                        }
                    }
                />
                <span
                    class=style::zoom_mark
                    aria-hidden="true"
                    style=move || format!("left: calc({}% - 1px)", along(zoom.get()) * 100.0)
                ></span>
                {stops
                    .get_value()
                    .into_iter()
                    .map(|stop| {
                        let place = along(stop) * 100.0;
                        // The least needed labels hide first as the bar narrows.
                        let drops = if Some(stop) == ends.0 || Some(stop) == ends.1 {
                            style::zoom_ends
                        } else if stop == 1.0 || stop == 5.0 {
                            style::zoom_first
                        } else {
                            style::zoom_then
                        };
                        let placed = if place < 4.0 {
                            "left: 3px".to_owned()
                        } else if place > 96.0 {
                            "right: 3px".to_owned()
                        } else {
                            format!("left: {place}%; transform: translateX(-50%)")
                        };

                        view! {
                            <span
                                class=classes!(style::zoom_stop, drops)
                                class=(style::on, move || (zoom.get() - stop).abs() < 0.05)
                                aria-hidden="true"
                                data-zoom=stop.to_string()
                                style=placed
                            >
                                {format!("{}×", zoom_text(stop).trim_start_matches('0'))}
                            </span>
                        }
                    })
                    .collect_view()}
            </div>
            <code>{move || format!("{}×", zoom_text(zoom.get()))}</code>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zoom_reads_rounded() {
        assert_eq!(rounded(0.494), 0.49);
        assert_eq!(rounded(3.84), 3.8);
        assert_eq!(rounded(12.4), 12.0);
        assert_eq!(zoom_text(2.0), "2");
        assert_eq!(zoom_text(3.8), "3.8");
        assert_eq!(zoom_text(0.49), "0.49");
    }
}
