use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::app::Router;

/// `centres` holds the centre of each card in page order. Up and down go to
/// the nearest row above or below, and in it to the card nearest across.
fn step(centres: &[(f64, f64)], from: usize, key: &str) -> Option<usize> {
    let (x, y) = *centres.get(from)?;

    match key {
        "ArrowLeft" => from.checked_sub(1),
        "ArrowRight" => (from + 1 < centres.len()).then_some(from + 1),
        "ArrowUp" | "ArrowDown" => {
            let up = key == "ArrowUp";

            centres
                .iter()
                .enumerate()
                .filter(|(_, (_, at))| if up { *at < y - 1.0 } else { *at > y + 1.0 })
                .min_by(|(_, one), (_, other)| {
                    let rows = (one.1 - y).abs().total_cmp(&(other.1 - y).abs());

                    rows.then((one.0 - x).abs().total_cmp(&(other.0 - x).abs()))
                })
                .map(|(at, _)| at)
        }
        _ => None,
    }
}

/// A new device is only focused, since picking it would connect it.
// `router` is given by the key handler that calls this, which has no owner
// to read the context from.
pub(super) fn go(router: Router, key: &str) {
    let Ok(listed) = document().query_selector_all("[data-card]") else {
        return;
    };

    let cards: Vec<web_sys::HtmlElement> = (0..listed.length())
        .filter_map(|at| listed.item(at)?.dyn_into().ok())
        .collect();
    let focused = document().active_element();
    let Some(from) = cards
        .iter()
        .position(|card| focused.as_ref() == Some(&**card))
    else {
        return;
    };

    let centres: Vec<(f64, f64)> = cards
        .iter()
        .map(|card| {
            let rect = card.get_bounding_client_rect();

            (
                rect.x() + rect.width() / 2.0,
                rect.y() + rect.height() / 2.0,
            )
        })
        .collect();
    let Some(to) = step(&centres, from, key).and_then(|to| cards.get(to)) else {
        return;
    };

    let _ = to.focus();

    if to.has_attribute("data-new") {
        return;
    }

    if let Some(id) = to.get_attribute("data-card").and_then(|id| id.parse().ok()) {
        router.pick(Some(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_step_along_a_row_and_to_the_nearest_card_of_the_next() {
        let centres = [
            (0.0, 0.0),
            (10.0, 0.0),
            (20.0, 0.0),
            (0.0, 10.0),
            (10.0, 10.0),
            (20.0, 10.0),
            (12.0, 30.0),
        ];

        assert_eq!(step(&centres, 0, "ArrowLeft"), None);
        assert_eq!(step(&centres, 2, "ArrowRight"), Some(3));
        assert_eq!(step(&centres, 6, "ArrowRight"), None);
        assert_eq!(step(&centres, 1, "ArrowDown"), Some(4));
        assert_eq!(step(&centres, 5, "ArrowDown"), Some(6));
        assert_eq!(step(&centres, 6, "ArrowUp"), Some(4));
        assert_eq!(step(&centres, 0, "ArrowUp"), None);
        assert_eq!(step(&centres, 0, "Tab"), None);
    }
}
