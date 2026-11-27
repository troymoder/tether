/// Converts RGBA pixels to the camera's I420: the luma of each pixel, then
/// the two chroma planes with one value per two by two pixels. The
/// coefficients are BT.601 in limited range. `width` and `height` must be
/// even.
pub(super) fn as_camera_has(points: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut picture = vec![0; width * height * 3 / 2];
    let (light, colours) = picture.split_at_mut(width * height);
    let (blues, reds) = colours.split_at_mut(width * height / 4);

    for (light, point) in light.iter_mut().zip(points.as_chunks::<4>().0) {
        let red = i32::from(point[0]);
        let green = i32::from(point[1]);
        let blue = i32::from(point[2]);

        *light = (((66 * red + 129 * green + 25 * blue + 128) >> 8) + 16) as u8;
    }

    // Two rows at a time, averaging the colour of each two by two block.
    let pairs = points.chunks_exact(width * 8);

    for ((pair, blues), reds) in pairs
        .zip(blues.chunks_exact_mut(width / 2))
        .zip(reds.chunks_exact_mut(width / 2))
    {
        let (above, below) = pair.split_at(width * 4);
        let blocks = above
            .as_chunks::<8>()
            .0
            .iter()
            .zip(below.as_chunks::<8>().0);

        for (((above, below), blueness), redness) in blocks.zip(blues).zip(reds) {
            let mean = |at: usize| {
                (i32::from(above[at])
                    + i32::from(above[at + 4])
                    + i32::from(below[at])
                    + i32::from(below[at + 4])
                    + 2)
                    / 4
            };

            let red = mean(0);
            let green = mean(1);
            let blue = mean(2);

            *blueness = (((-38 * red - 74 * green + 112 * blue + 128) >> 8) + 128) as u8;
            *redness = (((112 * red - 94 * green - 18 * blue + 128) >> 8) + 128) as u8;
        }
    }

    picture
}

/// The number of pictures packed against the previous one before a picture
/// is packed on its own again.
const ALONE_EVERY: u32 = 120;

#[derive(Default)]
pub(super) struct Packing {
    /// The picture packed last and its size, which the next is packed
    /// against.
    before: Option<(Vec<u8>, u32, u32)>,
    /// The pictures packed against a previous one since one was packed on
    /// its own.
    after: u32,
}

impl Packing {
    pub(super) fn pack(&mut self, picture: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
        let before = self
            .before
            .as_ref()
            .filter(|(_, wide, high)| (*wide, *high) == (width, height) && self.after < ALONE_EVERY)
            .map(|(before, ..)| before.as_slice());
        let packed = protocol::pack(picture.clone(), before, width as usize, height as usize);

        self.after = if before.is_some() { self.after + 1 } else { 0 };
        self.before = Some((picture, width, height));

        packed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_after_the_picture_before_and_now_and_then_alone() {
        let picture = vec![10, 12, 250, 3, 0, 0, 255, 1, 128, 130, 127, 0];
        let mut packing = Packing::default();

        let first = packing.pack(picture.clone(), 4, 2);

        assert_eq!(protocol::unpack(&first, None, 4, 2), Some(picture.clone()));

        let second = packing.pack(picture.clone(), 4, 2);

        assert_eq!(protocol::unpack(&second, None, 4, 2), None);
        assert_eq!(
            protocol::unpack(&second, Some(&picture), 4, 2),
            Some(picture.clone())
        );

        let other = packing.pack(picture.clone(), 2, 4);

        assert!(protocol::unpack(&other, None, 2, 4).is_some());

        packing.after = ALONE_EVERY;

        let again = packing.pack(picture.clone(), 2, 4);

        assert!(protocol::unpack(&again, None, 2, 4).is_some());
    }

    #[test]
    fn makes_colours_as_a_camera_has_them() {
        // Two by two of white, then of black, in limited range.
        let white = as_camera_has(&[255; 16], 2, 2);

        assert_eq!(white, [235, 235, 235, 235, 128, 128]);

        let black = as_camera_has(&[0, 0, 0, 255].repeat(4), 2, 2);

        assert_eq!(black, [16, 16, 16, 16, 128, 128]);

        // The chroma averages all four pixels. Red at the top left and the
        // rest black gives a quarter of red.
        let mut corner = [0, 0, 0, 255].repeat(4);

        corner[0] = 255;

        assert_eq!(as_camera_has(&corner, 2, 2), [82, 16, 16, 16, 119, 156]);
    }
}
