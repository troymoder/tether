use leptos::prelude::*;
use stylance::classes;

use crate::ui;

pub(crate) fn matches(sought: &str, texts: &[&str]) -> bool {
    let sought = sought.trim().to_lowercase();

    sought.is_empty()
        || texts
            .iter()
            .any(|text| text.to_lowercase().contains(&sought))
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn SearchBox(
    sought: RwSignal<String>,
    label: &'static str,
    placeholder: &'static str,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    view! {
        <input
            type="search"
            class=class
            aria-label=label
            placeholder=placeholder
            bind:value=sought
        />
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Empty(
    #[prop(into)] all: Signal<usize>,
    #[prop(into)] shown: Signal<usize>,
    none: &'static str,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    let said = move || {
        if all.get() == 0 {
            Some(none)
        } else if shown.get() == 0 {
            Some("None match.")
        } else {
            None
        }
    };

    move || {
        said().map(
            |said| view! { <p class=classes!(ui::style::hint, ui::style::said, class)>{said}</p> },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sought_text_matches_in_any_case() {
        assert!(matches("pix", &["Pixel 8", "emulator-5554"]));
        assert!(matches("  5554 ", &["Pixel 8", "emulator-5554"]));
        assert!(matches("", &["Pixel 8"]));
        assert!(matches("   ", &[]));
        assert!(!matches("iphone", &["Pixel 8", "emulator-5554"]));
        assert!(!matches("pixel", &[]));
    }
}
