use android::tweaks::Tweak;
use leptos::prelude::*;
use stylance::classes;

use super::{style, use_tweak};

/// Android's font scale steps, in percent.
const SIZES: [(u16, &str); 6] = [
    (85, "Small"),
    (100, "Default"),
    (115, "Large"),
    (130, "Largest"),
    (150, "150%"),
    (200, "200%"),
];

/// A size set by hand on the device shows as the nearest step.
fn nearest(size: u16) -> usize {
    let mut nearest = 0;

    for (index, (step, _)) in SIZES.iter().enumerate() {
        if step.abs_diff(size) < SIZES[nearest].0.abs_diff(size) {
            nearest = index;
        }
    }

    nearest
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn TextSize(size: Signal<u16>, again: Trigger) -> impl IntoView {
    let (busy, set) = use_tweak(again);
    let step = Memo::new(move |_| nearest(size.get()));

    view! {
        <label class=classes!(style::tweak, style::slide)>
            <span>"Size"</span>
            <input
                type="range"
                min="0"
                max=(SIZES.len() - 1).to_string()
                step="1"
                aria-label="Text size"
                prop:value=move || step.get().to_string()
                style:--at=move || format!("{}%", step.get() * 100 / (SIZES.len() - 1))
                disabled=busy
                on:change=move |event| {
                    let chosen = event_target_value(&event)
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| SIZES.get(index));

                    if let Some((size, _)) = chosen {
                        set.run(Tweak::TextSize(*size));
                    }
                }
            />
            <code>{move || format!("{}%", size.get())}</code>
        </label>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_set_by_hand_shows_as_the_nearest_step() {
        assert_eq!(nearest(100), 1);
        assert_eq!(nearest(0), 0);
        assert_eq!(nearest(120), 2);
        assert_eq!(nearest(140), 3);
        assert_eq!(nearest(500), 5);
    }
}
