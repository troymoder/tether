/// The first byte of a packed picture: `ALONE` for a keyframe, `AFTER` for
/// its difference from the previous picture.
const ALONE: u8 = 0;
const AFTER: u8 = 1;

/// A planar YUV 4:2:0 picture is the luma plane, `width` by `height`, then
/// the two chroma planes at half of each.
fn rows(picture: &mut [u8], width: usize, height: usize, mut each: impl FnMut(&mut [u8])) {
    let (light, colours) = picture.split_at_mut(width * height);

    light.chunks_exact_mut(width).for_each(&mut each);
    colours.chunks_exact_mut(width / 2).for_each(&mut each);
}

/// Given `before`, the picture the other side unpacked last, each byte is
/// stored as its change since then, which is zero for a still scene. Without
/// it, each byte is stored as its difference from the one to its left, so the
/// picture unpacks on its own. Pass no `before` for the first picture,
/// periodically after, and whenever the other side may have missed one.
/// `width` and `height` must be even, and the picture and `before` must match
/// them in length.
pub fn pack(mut picture: Vec<u8>, before: Option<&[u8]>, width: usize, height: usize) -> Vec<u8> {
    let kind = match before {
        Some(before) => {
            for (now, then) in picture.iter_mut().zip(before) {
                *now = now.wrapping_sub(*then);
            }

            AFTER
        }
        None => {
            rows(&mut picture, width, height, |row| {
                for at in (1..row.len()).rev() {
                    row[at] = row[at].wrapping_sub(row[at - 1]);
                }
            });

            ALONE
        }
    };

    // Level 1 is the fastest that still finds the repeats. Compressing into
    // memory cannot fail.
    let mut packed = vec![kind];

    packed.extend(zstd::bulk::compress(&picture, 1).expect("a picture that can be packed"));

    packed
}

/// The bytes come from the other side, so this returns None if they are not
/// a packed picture of this size, or if they were packed against a `before`
/// that is not given.
pub fn unpack(
    packed: &[u8],
    before: Option<&[u8]>,
    width: usize,
    height: usize,
) -> Option<Vec<u8>> {
    let (&kind, rest) = packed.split_first()?;
    let size = width.checked_mul(height)?.checked_mul(3)? / 2;

    if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return None;
    }

    // Capped at the expected size, whatever the frame header claims.
    let mut picture = zstd::bulk::decompress(rest, size).ok()?;

    if picture.len() != size {
        return None;
    }

    match (kind, before) {
        (AFTER, Some(before)) if before.len() == size => {
            for (now, then) in picture.iter_mut().zip(before) {
                *now = now.wrapping_add(*then);
            }
        }
        (ALONE, _) => rows(&mut picture, width, height, |row| {
            for at in 1..row.len() {
                row[at] = row[at].wrapping_add(row[at - 1]);
            }
        }),
        _ => return None,
    }

    Some(picture)
}

pub fn squeeze(bytes: &[u8]) -> Vec<u8> {
    // Compressing into memory cannot fail.
    zstd::bulk::compress(bytes, 1).expect("bytes that can be packed")
}

/// The bytes come from the other side, so any length but `size` gives None.
pub fn swell(packed: &[u8], size: usize) -> Option<Vec<u8>> {
    let bytes = zstd::bulk::decompress(packed, size).ok()?;

    (bytes.len() == size).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packed_picture_comes_back_as_it_was() {
        // Four by two: eight luma bytes, then two of each chroma plane.
        let picture: Vec<u8> = vec![10, 12, 250, 3, 0, 0, 255, 1, 128, 130, 127, 0];
        let packed = pack(picture.clone(), None, 4, 2);

        assert_eq!(unpack(&packed, None, 4, 2), Some(picture.clone()));

        let smooth: Vec<u8> = (0..640 * 480 * 3 / 2)
            .map(|at| (at % 640 / 4) as u8)
            .collect();

        assert!(pack(smooth.clone(), None, 640, 480).len() < smooth.len() / 20);
        assert_eq!(
            unpack(&pack(smooth.clone(), None, 640, 480), None, 640, 480),
            Some(smooth.clone())
        );

        let still = pack(smooth.clone(), Some(&smooth), 640, 480);

        assert!(still.len() < 100);
        assert_eq!(
            unpack(&still, Some(&smooth), 640, 480),
            Some(smooth.clone())
        );

        let moved: Vec<u8> = smooth.iter().map(|point| point.wrapping_add(3)).collect();
        let after = pack(moved.clone(), Some(&smooth), 640, 480);

        assert_eq!(unpack(&after, Some(&smooth), 640, 480), Some(moved));
        assert_eq!(unpack(&after, None, 640, 480), None);

        assert_eq!(unpack(&packed, None, 4, 4), None);
        assert_eq!(unpack(&packed, None, 2, 2), None);
        assert_eq!(unpack(&picture, None, 4, 2), None);
    }
}
