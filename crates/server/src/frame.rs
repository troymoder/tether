//! Frames a webcam picture from this machine as the device's camera sees it.
//! This has to match the page's capture worker (crates/web/src/link/worker.rs),
//! which does the same for what the page plays.
//!
//! Every picture is YUV 4:2:0 in three planes, one after another, without
//! padding, of an even width and height.

use protocol::Framing;

struct Picture {
    bytes: Vec<u8>,
    width: usize,
    height: usize,
}

impl Picture {
    fn black(width: usize, height: usize) -> Self {
        let mut bytes = vec![128; width * height * 3 / 2];
        bytes[..width * height].fill(16);

        Self {
            bytes,
            width,
            height,
        }
    }

    /// The offset of each of its three planes, and the length of their rows.
    fn planes(&self) -> [(usize, usize); 3] {
        let light = self.width * self.height;

        [
            (0, self.width),
            (light, self.width / 2),
            (light + light / 4, self.width / 2),
        ]
    }
}

/// On its side the device sees the whole camera. Upright it sees only the left
/// part, as much taller than wide as the camera is wider than tall.
fn strip(width: usize, height: usize, upright: bool) -> usize {
    if upright {
        (height * height / width / 2 * 2).max(2)
    } else {
        width
    }
}

/// Rounded down to even, so the half-size colour planes start at the same
/// place.
fn down(at: f32) -> usize {
    (at as usize) / 2 * 2
}

fn even(side: f32) -> usize {
    down(side).max(2)
}

pub(crate) fn framed(
    picture: Vec<u8>,
    (width, height): (usize, usize),
    framing: Framing,
    camera: (usize, usize),
    upright: bool,
) -> Option<Vec<u8>> {
    if picture.len() != width * height * 3 / 2
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
    {
        return None;
    }

    let mut now = Picture {
        bytes: picture,
        width,
        height,
    };

    if framing.mirrored {
        now = mirror(&now);
    }

    if !framing.quarters.is_multiple_of(4) {
        now = turn(&now, framing.quarters % 4);
    }

    let (camera_width, camera_height) = camera;
    let strip = strip(camera_width, camera_height, upright);
    let (left, top, wide, high) = framing.seen(
        now.width as f32 / now.height as f32,
        strip as f32 / camera_height as f32,
    );
    let part_width = even(wide * now.width as f32).min(now.width);
    let part_height = even(high * now.height as f32).min(now.height);
    let x = down(left * now.width as f32).min(now.width - part_width);
    let y = down(top * now.height as f32).min(now.height - part_height);
    let mut out = Picture::black(camera_width, camera_height);
    let [(from_y, row_y), (from_u, row_u), (from_v, row_v)] = now.planes();
    let [(to_y, out_y), (to_u, out_u), (to_v, out_v)] = out.planes();
    let at = |plane: usize, row: usize, x: usize, y: usize| plane + y * row + x;

    // SAFETY: each plane is read and written inside its bounds. The part lies
    // inside the picture, and the strip inside the camera's picture.
    unsafe {
        yuv_sys::rs_I420Scale(
            now.bytes.as_ptr().add(at(from_y, row_y, x, y)),
            row_y as i32,
            now.bytes.as_ptr().add(at(from_u, row_u, x / 2, y / 2)),
            row_u as i32,
            now.bytes.as_ptr().add(at(from_v, row_v, x / 2, y / 2)),
            row_v as i32,
            part_width as i32,
            part_height as i32,
            out.bytes.as_mut_ptr().add(to_y),
            out_y as i32,
            out.bytes.as_mut_ptr().add(to_u),
            out_u as i32,
            out.bytes.as_mut_ptr().add(to_v),
            out_v as i32,
            strip as i32,
            camera_height as i32,
            yuv_sys::FilterMode_kFilterBox,
        );
    }

    Some(out.bytes)
}

fn mirror(picture: &Picture) -> Picture {
    let mut out = Picture::black(picture.width, picture.height);
    let [(from_y, row_y), (from_u, row_u), (from_v, row_v)] = picture.planes();
    let [(to_y, _), (to_u, _), (to_v, _)] = out.planes();

    // SAFETY: both are of the same size, which the planes are within.
    unsafe {
        yuv_sys::rs_I420Mirror(
            picture.bytes.as_ptr().add(from_y),
            row_y as i32,
            picture.bytes.as_ptr().add(from_u),
            row_u as i32,
            picture.bytes.as_ptr().add(from_v),
            row_v as i32,
            out.bytes.as_mut_ptr().add(to_y),
            row_y as i32,
            out.bytes.as_mut_ptr().add(to_u),
            row_u as i32,
            out.bytes.as_mut_ptr().add(to_v),
            row_v as i32,
            picture.width as i32,
            picture.height as i32,
        );
    }

    out
}

/// `quarters` is 1 to 3, clockwise.
fn turn(picture: &Picture, quarters: u8) -> Picture {
    let on_its_side = quarters % 2 == 1;
    let mut out = if on_its_side {
        Picture::black(picture.height, picture.width)
    } else {
        Picture::black(picture.width, picture.height)
    };

    let [(from_y, row_y), (from_u, row_u), (from_v, row_v)] = picture.planes();
    let [(to_y, out_y), (to_u, out_u), (to_v, out_v)] = out.planes();
    let mode = match quarters {
        1 => yuv_sys::RotationMode_kRotate90,
        2 => yuv_sys::RotationMode_kRotate180,
        _ => yuv_sys::RotationMode_kRotate270,
    };

    // SAFETY: the turned picture has as many pixels as the picture, its sides
    // swapped for an odd number of quarters, and its planes follow its sides.
    unsafe {
        yuv_sys::rs_I420Rotate(
            picture.bytes.as_ptr().add(from_y),
            row_y as i32,
            picture.bytes.as_ptr().add(from_u),
            row_u as i32,
            picture.bytes.as_ptr().add(from_v),
            row_v as i32,
            out.bytes.as_mut_ptr().add(to_y),
            out_y as i32,
            out.bytes.as_mut_ptr().add(to_u),
            out_u as i32,
            out.bytes.as_mut_ptr().add(to_v),
            out_v as i32,
            picture.width as i32,
            picture.height as i32,
            mode,
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn made(width: usize, height: usize, light: impl Fn(usize, usize) -> u8) -> Vec<u8> {
        let mut picture = Picture::black(width, height);

        for y in 0..height {
            for x in 0..width {
                picture.bytes[y * width + x] = light(x, y);
            }
        }

        picture.bytes
    }

    fn light(picture: &[u8], width: usize, x: usize, y: usize) -> u8 {
        picture[y * width + x]
    }

    #[test]
    fn of_its_own_shape_it_is_the_picture() {
        let picture = made(8, 4, |x, y| (x * 10 + y) as u8);
        let out = framed(picture.clone(), (8, 4), Framing::default(), (8, 4), false).unwrap();

        assert_eq!(out, picture);
        assert!(framed(vec![0; 5], (8, 4), Framing::default(), (8, 4), false).is_none());
    }

    #[test]
    fn mirrors_and_turns_as_the_worker_does() {
        let picture = made(8, 4, |x, _| if x < 4 { 200 } else { 20 });
        let mirrored = Framing {
            mirrored: true,
            ..Framing::default()
        };
        let out = framed(picture.clone(), (8, 4), mirrored, (8, 4), false).unwrap();

        assert_eq!((light(&out, 8, 0, 0), light(&out, 8, 7, 0)), (20, 200));

        // A quarter turn clockwise puts the left at the top, for a camera as
        // tall as the picture was wide.
        let turned = Framing {
            quarters: 1,
            ..Framing::default()
        };
        let out = framed(picture, (8, 4), turned, (4, 8), false).unwrap();

        assert_eq!((light(&out, 4, 0, 0), light(&out, 4, 0, 7)), (200, 20));
    }

    #[test]
    fn fills_the_strip_and_leaves_the_rest_black() {
        // Upright, a camera eight by four sees a strip two wide, from the
        // middle of the picture, starting at an even pixel for the colours.
        let picture = made(8, 4, |x, _| if (2..6).contains(&x) { 200 } else { 20 });
        let out = framed(picture, (8, 4), Framing::default(), (8, 4), true).unwrap();

        assert_eq!(light(&out, 8, 0, 0), 200);
        assert_eq!(light(&out, 8, 1, 3), 200);
        assert_eq!(light(&out, 8, 2, 0), 16);
        assert_eq!(light(&out, 8, 7, 3), 16);
    }
}
