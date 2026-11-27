//! scrcpy's control messages. All fields are big-endian.

use std::fmt::Debug;

use adb::Channel;
use adb::libadb::Session;
use anyhow::Result;

use super::keys::{button_code, key_code};

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// `x` and `y` are in pixels of the shown video frame, which was `width`
    /// by `height`.
    Touch {
        action: TouchAction,
        pointer: u32,
        x: f32,
        y: f32,
        width: u16,
        height: u16,
    },

    Key {
        down: bool,
        /// `KeyboardEvent.code`.
        code: String,
        shift: bool,
        ctrl: bool,
        alt: bool,
    },

    Text(String),
    /// `dx` and `dy` are in mouse wheel notches, positive to the right and
    /// up.
    Scroll {
        x: f32,
        y: f32,
        width: u16,
        height: u16,
        dx: f32,
        dy: f32,
    },

    Button(Button),
    /// `repeat` counts from 1 while the button stays down, and is 0
    /// otherwise. Only buttons the device auto-repeats are sent as still
    /// down, because it treats a repeat of any other button as a new press.
    Hold {
        button: Button,
        down: bool,
        repeat: u32,
    },

    /// The text arrives on [`Replies`](super::Replies).
    ReadClipboard,
    Clipboard(String),
    /// Makes the encoder restart with a key frame, for a viewer that joins
    /// the video partway.
    ResetVideo,
    Torch(bool),
    ZoomIn,
    ZoomOut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TouchAction {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Button {
    Back,
    Home,
    Recents,
    VolumeUp,
    VolumeDown,
    Power,
}

pub struct Control<S> {
    channel: Channel<S>,
}

impl<S> Control<S>
where
    S: Session,
    S::IoError: Debug,
{
    pub(super) fn new(channel: Channel<S>) -> Self {
        Control { channel }
    }

    pub async fn send(&mut self, input: &Input) -> Result<()> {
        self.write(&control_message(input)).await
    }

    pub async fn write(&mut self, messages: &[u8]) -> Result<()> {
        self.channel.write(messages).await
    }
}

/// The page encodes these for tetherd, which forwards them unchanged.
pub fn control_message(input: &Input) -> Vec<u8> {
    let mut out = Vec::new();

    match input {
        Input::Touch {
            action,
            pointer,
            x,
            y,
            width,
            height,
        } => {
            let lifted = matches!(action, TouchAction::Up | TouchAction::Cancel);

            out.push(2);
            out.push(match action {
                TouchAction::Down => 0,
                TouchAction::Up => 1,
                TouchAction::Move => 2,
                TouchAction::Cancel => 3,
            });

            out.extend(u64::from(*pointer).to_be_bytes());
            position(&mut out, *x, *y, *width, *height);
            // Pressure, where 0xffff is a full press.
            out.extend(if lifted { 0u16 } else { 0xffff }.to_be_bytes());
            // No mouse buttons, because this is a finger.
            out.extend([0; 8]);
        }
        Input::Key {
            down,
            code,
            shift,
            ctrl,
            alt,
        } => {
            if let Some(code) = key_code(code) {
                // Android's META_SHIFT_ON, META_CTRL_ON and META_ALT_ON.
                let meta = i32::from(*shift) | i32::from(*ctrl) << 12 | i32::from(*alt) << 1;

                key(&mut out, *down, code, 0, meta);
            }
        }
        Input::Text(text) => {
            let mut rest = text.as_str();

            // scrcpy accepts at most 300 bytes of text per message.
            while !rest.is_empty() {
                let mut end = rest.len().min(300);

                while !rest.is_char_boundary(end) {
                    end -= 1;
                }

                out.push(1);
                out.extend((end as u32).to_be_bytes());
                out.extend(&rest.as_bytes()[..end]);
                rest = &rest[end..];
            }
        }
        Input::Scroll {
            x,
            y,
            width,
            height,
            dx,
            dy,
        } => {
            out.push(3);
            position(&mut out, *x, *y, *width, *height);
            out.extend(notches(*dx).to_be_bytes());
            out.extend(notches(*dy).to_be_bytes());
            out.extend([0; 4]);
        }
        Input::ReadClipboard => {
            // Copy key 0 reads the clipboard without sending a copy key first.
            out.extend([8, 0]);
        }
        Input::Clipboard(text) => {
            // scrcpy accepts a message of at most 256 KiB.
            let mut end = text.len().min(250_000);

            while !text.is_char_boundary(end) {
                end -= 1;
            }

            out.push(9);
            // Sequence 0 asks for no acknowledgement, and paste is off.
            out.extend(0u64.to_be_bytes());
            out.push(0);
            out.extend((end as u32).to_be_bytes());
            out.extend(&text.as_bytes()[..end]);
        }
        Input::Button(button) => {
            key(&mut out, true, button_code(*button), 0, 0);
            key(&mut out, false, button_code(*button), 0, 0);
        }
        Input::Hold {
            button,
            down,
            repeat,
        } => key(&mut out, *down, button_code(*button), *repeat, 0),
        Input::ResetVideo => out.push(17),
        Input::Torch(on) => out.extend([18, u8::from(*on)]),
        Input::ZoomIn => out.push(19),
        Input::ZoomOut => out.push(20),
    }

    out
}

/// scrcpy ignores the event if the size differs from the one it is
/// streaming.
fn position(out: &mut Vec<u8>, x: f32, y: f32, width: u16, height: u16) {
    out.extend((x as i32).to_be_bytes());
    out.extend((y as i32).to_be_bytes());
    out.extend(width.to_be_bytes());
    out.extend(height.to_be_bytes());
}

/// `repeat` counts how often a held key has been sent as down since it was
/// pressed, because Android does not repeat injected keys.
fn key(out: &mut Vec<u8>, down: bool, code: i32, repeat: u32, meta: i32) {
    out.push(0);
    out.push(if down { 0 } else { 1 });
    out.extend(code.to_be_bytes());
    out.extend(repeat.to_be_bytes());
    out.extend(meta.to_be_bytes());
}

/// scrcpy encodes a scroll of up to 16 notches as a signed 16-bit fraction.
fn notches(count: f32) -> i16 {
    ((count / 16.0).clamp(-1.0, 1.0) * 32768.0).clamp(-32768.0, 32767.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_touch() {
        let message = control_message(&Input::Touch {
            action: TouchAction::Move,
            pointer: 0x0102,
            x: 300.7,
            y: 2.0,
            width: 720,
            height: 1280,
        });

        #[rustfmt::skip]
        assert_eq!(message, [
            2, 2,
            0, 0, 0, 0, 0, 0, 1, 2,
            0, 0, 1, 44, 0, 0, 0, 2,
            2, 208, 5, 0,
            0xff, 0xff,
            0, 0, 0, 0, 0, 0, 0, 0,
        ]);

        assert_eq!(message.len(), 32);
    }

    #[test]
    fn writes_keys() {
        let message = control_message(&Input::Key {
            down: false,
            code: "KeyC".to_owned(),
            shift: true,
            ctrl: true,
            alt: false,
        });

        assert_eq!(message, [0, 1, 0, 0, 0, 31, 0, 0, 0, 0, 0, 0, 0x10, 1]);

        let back = control_message(&Input::Button(Button::Back));

        assert_eq!(back.len(), 28);
        assert_eq!(back[..6], [0, 0, 0, 0, 0, 4]);
        assert_eq!(back[14..20], [0, 1, 0, 0, 0, 4]);

        let held = |down: bool, repeat: u32| {
            control_message(&Input::Hold {
                button: Button::VolumeUp,
                down,
                repeat,
            })
        };

        assert_eq!(held(true, 0), [0, 0, 0, 0, 0, 24, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(held(true, 3), [0, 0, 0, 0, 0, 24, 0, 0, 0, 3, 0, 0, 0, 0]);
        assert_eq!(held(false, 0), [0, 1, 0, 0, 0, 24, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn writes_text_and_scroll() {
        assert_eq!(
            control_message(&Input::Text("hé".to_owned())),
            [1, 0, 0, 0, 3, b'h', 0xc3, 0xa9]
        );

        // 301 bytes, with a two-byte character across the limit.
        let long = format!("{}é", "a".repeat(299));
        let message = control_message(&Input::Text(long));

        assert_eq!(message[..5], [1, 0, 0, 1, 43]);
        assert_eq!(message[304..], [1, 0, 0, 0, 2, 0xc3, 0xa9]);

        let scroll = control_message(&Input::Scroll {
            x: 1.0,
            y: 2.0,
            width: 3,
            height: 4,
            dx: 0.0,
            dy: -1.0,
        });

        assert_eq!(scroll.len(), 21);
        assert_eq!(scroll[13..17], [0, 0, 0xf8, 0]);

        assert_eq!(control_message(&Input::ReadClipboard), [8, 0]);
        assert_eq!(control_message(&Input::Torch(true)), [18, 1]);
        assert_eq!(control_message(&Input::ZoomOut), [20]);
        assert_eq!(
            control_message(&Input::Clipboard("hé".to_owned())),
            [9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, b'h', 0xc3, 0xa9]
        );
    }
}
