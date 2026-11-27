//! Shapes are width over height.

use protocol::Framing;

pub(super) fn clamped(next: Framing) -> Framing {
    Framing {
        zoom: next.zoom.clamp(1.0, Framing::CLOSEST),
        x: next.x.clamp(0.0, 1.0),
        y: next.y.clamp(0.0, 1.0),
        ..next
    }
}

pub(super) fn given_shape((width, height): (u32, u32), quarters: u8) -> f32 {
    let shape = width as f32 / height as f32;

    if quarters % 2 == 1 {
        1.0 / shape
    } else {
        shape
    }
}

/// On its side the source is as wide as the pad is high.
pub(super) fn turned_style(framing: Framing, given: f32) -> String {
    let Framing {
        quarters, mirrored, ..
    } = framing;
    let (wide, high) = if quarters % 2 == 1 {
        (100.0 / given, 100.0 * given)
    } else {
        (100.0, 100.0)
    };

    format!(
        "width: {wide}%; height: {high}%; transform: translate(-50%, -50%) rotate({}deg) scaleX({})",
        u32::from(quarters) * 90,
        if mirrored { -1 } else { 1 },
    )
}

/// `now` zoomed so that the seen part's right edge lands at `x`, a part of
/// the pad's width, while its top left corner stays where it is.
pub(super) fn resized(now: Framing, given: f32, camera: f32, x: f32) -> Framing {
    let (left, top, ..) = now.seen(given, camera);
    let (_, _, widest, highest) = Framing { zoom: 1.0, ..now }.seen(given, camera);
    let zoom = (widest / (x - left).max(0.01)).clamp(1.0, Framing::CLOSEST);

    Framing {
        zoom,
        x: left + widest / zoom / 2.0,
        y: top + highest / zoom / 2.0,
        ..now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_source_is_higher_than_wide() {
        assert_eq!(given_shape((1600, 900), 0), 1600.0 / 900.0);
        assert_eq!(given_shape((1600, 900), 1), 900.0 / 1600.0);
    }

    #[test]
    fn a_framing_stays_in_range() {
        let wild = Framing {
            zoom: 20.0,
            x: -1.0,
            y: 2.0,
            ..Framing::default()
        };

        let held = clamped(wild);

        assert_eq!((held.zoom, held.x, held.y), (Framing::CLOSEST, 0.0, 1.0));
    }

    #[test]
    fn resizing_keeps_the_top_left_corner() {
        let whole = Framing::default();
        let half = resized(whole, 1.0, 1.0, 0.5);

        assert_eq!(half.zoom, 2.0);
        assert_eq!(half.seen(1.0, 1.0), (0.0, 0.0, 0.5, 0.5));
    }
}
