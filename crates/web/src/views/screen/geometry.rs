//! Angles are in degrees counter-clockwise from upright.

use super::playing::Size;

/// An upright phone, drawn until a picture of the screen gives the real
/// shape.
pub(super) const SHAPE: Size = (1080, 2400);

/// The Rotate button skips upside down, since phones do not draw their screen
/// that way.
pub(super) fn next_turn(from: u16) -> u16 {
    match from {
        0 => 270,
        270 => 90,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_cycles_without_upside_down() {
        assert_eq!(next_turn(0), 270);
        assert_eq!(next_turn(270), 90);
        assert_eq!(next_turn(90), 0);
        assert_eq!(next_turn(180), 0);
    }
}
