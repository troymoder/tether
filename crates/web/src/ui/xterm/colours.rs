use leptos::prelude::*;
use wasm_bindgen::JsValue;

fn token(name: &str) -> Option<String> {
    let root = document().document_element()?;
    let style = window().get_computed_style(&root).ok().flatten()?;
    let value = style.get_property_value(name).ok()?;
    let value = value.trim();

    (!value.is_empty()).then(|| value.to_owned())
}

pub(crate) fn token_number(name: &str) -> Option<u32> {
    u32::from_str_radix(token(name)?.trim_start_matches('#'), 16).ok()
}

/// The sixteen ANSI colours by xterm.js theme key.
const NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "brightBlack",
    "brightRed",
    "brightGreen",
    "brightYellow",
    "brightBlue",
    "brightMagenta",
    "brightCyan",
    "brightWhite",
];

const DARK: [&str; 16] = [
    "#2a2a2a", "#ff6b63", "#3fb950", "#e3b341", "#6ea8ff", "#c792ea", "#56c8d8", "#cfcfcf",
    "#6b6b6b", "#ff8f88", "#6fdd8b", "#f2cc60", "#9cc4ff", "#ddb3f8", "#8be0ec", "#ffffff",
];

const LIGHT: [&str; 16] = [
    "#1a1a1a", "#c62828", "#1a7f37", "#8a5a00", "#1f5fbf", "#8e24aa", "#00838f", "#5c5c5c",
    "#8a8a8a", "#d84343", "#2e9950", "#a87000", "#3a78d8", "#a64dc0", "#1a9aa6", "#000000",
];

/// True for a `#rrggbb` colour whose channels sum past half of their maximum.
fn light(colour: &str) -> bool {
    u32::from_str_radix(colour.trim_start_matches('#'), 16)
        .is_ok_and(|colour| (colour >> 16 & 0xff) + (colour >> 8 & 0xff) + (colour & 0xff) > 384)
}

pub(crate) fn colours() -> JsValue {
    let ground = token("--raised").unwrap_or_else(|| "#0c0c0c".to_owned());
    let text = token("--text").unwrap_or_else(|| "#e6e6e6".to_owned());

    let theme = js_sys::Object::new();
    let set = |name: &str, value: &str| {
        let _ = js_sys::Reflect::set(&theme, &name.into(), &value.into());
    };

    set("background", &ground);
    set("foreground", &text);
    set("cursor", &text);
    set("cursorAccent", &ground);
    // The alpha suffix lets the background show through the selection.
    set(
        "selectionBackground",
        &format!(
            "{}66",
            token("--accent-fill").unwrap_or_else(|| "#2f6fd6".to_owned())
        ),
    );

    for (name, colour) in NAMES.iter().zip(if light(&ground) { LIGHT } else { DARK }) {
        set(name, colour);
    }

    theme.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ground_is_light_past_half_brightness() {
        assert!(light("#ffffff"));
        assert!(light("#f0f0f0"));
        assert!(!light("#0c0c0c"));
        assert!(!light("#808080"));
        assert!(!light("not a colour"));
    }
}
