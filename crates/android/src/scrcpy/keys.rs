use super::Button;

/// Android's `KEYCODE_*` values.
pub(super) fn button_code(button: Button) -> i32 {
    match button {
        Button::Back => 4,
        Button::Home => 3,
        Button::Recents => 187,
        Button::VolumeUp => 24,
        Button::VolumeDown => 25,
        Button::Power => 26,
    }
}

pub(super) fn key_code(code: &str) -> Option<i32> {
    if let Some(letter) = code.strip_prefix("Key") {
        return single(letter, b'A'..=b'Z', 29);
    }

    if let Some(digit) = code.strip_prefix("Digit") {
        return single(digit, b'0'..=b'9', 7);
    }

    if let Some(digit) = code.strip_prefix("Numpad")
        && let Some(code) = single(digit, b'0'..=b'9', 144)
    {
        return Some(code);
    }

    if let Some(number) = code.strip_prefix('F')
        && let Ok(number @ 1..=12) = number.parse::<i32>()
    {
        return Some(130 + number);
    }

    Some(match code {
        "ArrowUp" => 19,
        "ArrowDown" => 20,
        "ArrowLeft" => 21,
        "ArrowRight" => 22,
        "Comma" => 55,
        "Period" => 56,
        "AltLeft" => 57,
        "AltRight" => 58,
        "ShiftLeft" => 59,
        "ShiftRight" => 60,
        "Tab" => 61,
        "Space" => 62,
        "Enter" => 66,
        "Backspace" => 67,
        "Backquote" => 68,
        "Minus" => 69,
        "Equal" => 70,
        "BracketLeft" => 71,
        "BracketRight" => 72,
        "Backslash" => 73,
        "Semicolon" => 74,
        "Quote" => 75,
        "Slash" => 76,
        "ContextMenu" => 82,
        "PageUp" => 92,
        "PageDown" => 93,
        "Escape" => 111,
        "Delete" => 112,
        "ControlLeft" => 113,
        "ControlRight" => 114,
        "CapsLock" => 115,
        "ScrollLock" => 116,
        "MetaLeft" => 117,
        "MetaRight" => 118,
        "PrintScreen" => 120,
        "Pause" => 121,
        "Home" => 122,
        "End" => 123,
        "Insert" => 124,
        "NumLock" => 143,
        "NumpadDivide" => 154,
        "NumpadMultiply" => 155,
        "NumpadSubtract" => 156,
        "NumpadAdd" => 157,
        "NumpadDecimal" => 158,
        "NumpadComma" => 159,
        "NumpadEnter" => 160,
        "NumpadEqual" => 161,
        _ => return None,
    })
}

fn single(name: &str, range: std::ops::RangeInclusive<u8>, first: i32) -> Option<i32> {
    match name.as_bytes() {
        [byte] if range.contains(byte) => Some(first + i32::from(byte - range.start())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_keyboard() {
        assert_eq!(key_code("KeyA"), Some(29));
        assert_eq!(key_code("KeyZ"), Some(54));
        assert_eq!(key_code("Digit0"), Some(7));
        assert_eq!(key_code("Numpad9"), Some(153));
        assert_eq!(key_code("NumpadEnter"), Some(160));
        assert_eq!(key_code("F1"), Some(131));
        assert_eq!(key_code("F12"), Some(142));
        assert_eq!(key_code("F13"), None);
        assert_eq!(key_code("Keyé"), None);
        assert_eq!(key_code("Fn"), None);
    }
}
