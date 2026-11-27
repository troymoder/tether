use protocol::Framing;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Job {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) fps: u8,
    pub(super) upright: bool,
    pub(super) lossless: bool,
    pub(super) framing: Framing,
}

impl Job {
    /// The width of the part of the frame the device sees. A device on its
    /// side sees all of it. An upright one sees a strip at the left that is
    /// as much taller than wide as the frame is wider than tall, at an even
    /// width so the chroma planes can be halved.
    pub(super) fn strip(self) -> u32 {
        if self.upright {
            (self.height * self.height / self.width / 2 * 2).max(2)
        } else {
            self.width
        }
    }

    /// The canvas transform (`a, b, c, d, e, f`) that draws a picture `wide`
    /// by `high` mirrored, rotated and scaled, so the framed part fills the
    /// strip the device sees.
    pub(super) fn transform(self, wide: f64, high: f64) -> [f64; 6] {
        let Framing {
            quarters, mirrored, ..
        } = self.framing;
        let (cos, sin) =
            [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)][usize::from(quarters % 4)];
        let mirror = if mirrored { -1.0 } else { 1.0 };
        let (turned_wide, turned_high) = if quarters % 2 == 1 {
            (high, wide)
        } else {
            (wide, high)
        };

        let strip = f64::from(self.strip());
        let height = f64::from(self.height);
        let (left, top, width, tall) = self
            .framing
            .seen((turned_wide / turned_high) as f32, (strip / height) as f32);
        let left = f64::from(left);
        let top = f64::from(top);
        let width = f64::from(width);
        let tall = f64::from(tall);
        let across = strip / (width * turned_wide);
        let down = height / (tall * turned_high);

        // Around its middle: mirrored, then rotated clockwise, then the
        // framed part moved to the corner and scaled to fill the strip.
        [
            across * cos * mirror,
            down * sin * mirror,
            -across * sin,
            down * cos,
            across
                * (-cos * mirror * wide / 2.0 + sin * high / 2.0 + turned_wide / 2.0
                    - left * turned_wide),
            down * (-sin * mirror * wide / 2.0 - cos * high / 2.0 + turned_high / 2.0
                - top * turned_high),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(job: Job, (wide, high): (f64, f64), (x, y): (f64, f64)) -> (f64, f64) {
        let [a, b, c, d, e, f] = job.transform(wide, high);

        (a * x + c * y + e, b * x + d * y + f)
    }

    fn job(width: u32, height: u32, upright: bool, framing: Framing) -> Job {
        Job {
            width,
            height,
            fps: 30,
            upright,
            lossless: true,
            framing,
        }
    }

    #[test]
    fn frames_a_picture() {
        let whole = Framing::default();

        // At the frame's own size, the picture is the frame.
        let same = job(8, 4, false, whole);

        assert_eq!(put(same, (8.0, 4.0), (0.0, 0.0)), (0.0, 0.0));
        assert_eq!(put(same, (8.0, 4.0), (8.0, 4.0)), (8.0, 4.0));

        // A square frame of a picture twice as wide as tall shows the middle
        // half at full height.
        let square = job(4, 4, false, whole);

        assert_eq!(put(square, (8.0, 4.0), (2.0, 0.0)), (0.0, 0.0));
        assert_eq!(put(square, (8.0, 4.0), (6.0, 4.0)), (4.0, 4.0));

        // Zoomed in twice at the right, it shows the right half at twice the
        // size.
        let close = job(
            8,
            4,
            false,
            Framing {
                zoom: 2.0,
                x: 1.0,
                y: 0.5,
                ..whole
            },
        );

        assert_eq!(put(close, (8.0, 4.0), (4.0, 1.0)), (0.0, 0.0));
        assert_eq!(put(close, (8.0, 4.0), (8.0, 3.0)), (8.0, 4.0));

        // Upright, the device sees a strip at the left that is taller than
        // wide, out of the middle of the picture.
        let upright = job(1440, 1080, true, whole);

        assert_eq!(upright.strip(), 810);
        assert_eq!(job(8, 4, true, whole).strip(), 2);
        assert_eq!(
            put(job(8, 4, true, whole), (8.0, 4.0), (3.0, 0.0)),
            (0.0, 0.0)
        );

        assert_eq!(
            put(job(8, 4, true, whole), (8.0, 4.0), (5.0, 4.0)),
            (2.0, 4.0)
        );
    }

    #[test]
    fn turns_a_picture() {
        let turned = |quarters: u8, mirrored: bool| {
            job(
                if quarters % 2 == 1 { 2 } else { 4 },
                if quarters % 2 == 1 { 4 } else { 2 },
                false,
                Framing {
                    quarters,
                    mirrored,
                    ..Framing::default()
                },
            )
        };

        let top_left = (0.0, 0.0);
        let bottom_left = (0.0, 2.0);

        assert_eq!(put(turned(0, true), (4.0, 2.0), top_left), (4.0, 0.0));
        assert_eq!(put(turned(2, false), (4.0, 2.0), top_left), (4.0, 2.0));
        // A quarter turn clockwise moves the bottom left corner to the top
        // left, and the top left corner to the top right.
        assert_eq!(put(turned(1, false), (4.0, 2.0), bottom_left), (0.0, 0.0));
        assert_eq!(put(turned(1, false), (4.0, 2.0), top_left), (2.0, 0.0));
        assert_eq!(put(turned(3, false), (4.0, 2.0), top_left), (0.0, 4.0));
    }
}
