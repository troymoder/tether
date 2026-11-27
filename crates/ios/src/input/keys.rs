//! A browser's `KeyboardEvent.code` as a USB HID usage of the keyboard page
//! (HID Usage Tables 1.5, section 10), which the phone's keyboard takes.

pub(super) fn usage(code: &str) -> Option<u16> {
    if let Some(letter) = code.strip_prefix("Key").and_then(single) {
        return letter
            .is_ascii_uppercase()
            .then(|| 0x04 + u16::from(letter as u8 - b'A'));
    }

    if let Some(digit) = code.strip_prefix("Digit").and_then(single) {
        // 1 to 9 come first and 0 last.
        return match digit {
            '0' => Some(0x27),
            '1'..='9' => Some(0x1e + u16::from(digit as u8 - b'1')),
            _ => None,
        };
    }

    if let Some(number) = code
        .strip_prefix('F')
        .and_then(|rest| rest.parse::<u16>().ok())
    {
        return (1..=12).contains(&number).then(|| 0x3a + number - 1);
    }

    let usage = match code {
        "Enter" | "NumpadEnter" => 0x28,
        "Escape" => 0x29,
        "Backspace" => 0x2a,
        "Tab" => 0x2b,
        "Space" => 0x2c,
        "Minus" => 0x2d,
        "Equal" => 0x2e,
        "BracketLeft" => 0x2f,
        "BracketRight" => 0x30,
        "Backslash" => 0x31,
        "Semicolon" => 0x33,
        "Quote" => 0x34,
        "Backquote" => 0x35,
        "Comma" => 0x36,
        "Period" => 0x37,
        "Slash" => 0x38,
        "CapsLock" => 0x39,
        "Insert" => 0x49,
        "Home" => 0x4a,
        "PageUp" => 0x4b,
        "Delete" => 0x4c,
        "End" => 0x4d,
        "PageDown" => 0x4e,
        "ArrowRight" => 0x4f,
        "ArrowLeft" => 0x50,
        "ArrowDown" => 0x51,
        "ArrowUp" => 0x52,
        "ControlLeft" => 0xe0,
        "ShiftLeft" => 0xe1,
        "AltLeft" => 0xe2,
        "MetaLeft" => 0xe3,
        "ControlRight" => 0xe4,
        "ShiftRight" => 0xe5,
        "AltRight" => 0xe6,
        "MetaRight" => 0xe7,
        _ => return None,
    };

    Some(usage)
}

fn single(rest: &str) -> Option<char> {
    let mut chars = rest.chars();
    let only = chars.next()?;

    chars.next().is_none().then_some(only)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_map_to_keyboard_usages() {
        assert_eq!(usage("KeyA"), Some(0x04));
        assert_eq!(usage("KeyZ"), Some(0x1d));
        assert_eq!(usage("Digit1"), Some(0x1e));
        assert_eq!(usage("Digit0"), Some(0x27));
        assert_eq!(usage("F1"), Some(0x3a));
        assert_eq!(usage("F12"), Some(0x45));
        assert_eq!(usage("Enter"), Some(0x28));
        assert_eq!(usage("ShiftLeft"), Some(0xe1));
        assert_eq!(usage("F13"), None);
        assert_eq!(usage("KeyAB"), None);
        assert_eq!(usage("MediaPlayPause"), None);
    }
}
