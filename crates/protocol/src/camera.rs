use rkyv::{Archive, Deserialize, Serialize};

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Lens {
    #[default]
    Back,
    Front,
}

impl Lens {
    pub const ALL: [Lens; 2] = [Lens::Back, Lens::Front];

    pub fn at(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Lens::Back => "back",
            Lens::Front => "front",
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct VirtualCamera {
    pub lens: Lens,
    pub settings: CameraSettings,
}

/// Android reads these only on a cold boot, so tetherd drops the quick-boot
/// snapshot when they change (crates/server/src/emulator/avd.rs).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct CameraSettings {
    pub width: u16,
    pub height: u16,
    pub fps: u8,
}

pub const CAMERA_RATES: [u8; 2] = [30, 60];

pub fn usual_camera(lens: Lens) -> CameraSettings {
    match lens {
        Lens::Back => CameraSettings {
            width: 1440,
            height: 1080,
            fps: 30,
        },
        Lens::Front => CameraSettings {
            width: 1280,
            height: 720,
            fps: 30,
        },
    }
}

// All landscape and no larger, because the emulator's camera failed to start
// with a portrait size or with 1920 by 1440.
pub const CAMERA_SIZES: [(u16, u16); 4] = [(640, 480), (1280, 720), (1440, 1080), (1920, 1080)];

/// Width over height. The camera is landscape, like a phone's sensor. A
/// device in landscape sees all of it. An upright device sees a portrait
/// crop of the inverse ratio, which the emulator cuts from the picture's left.
pub fn camera_shape((wide, high): (u16, u16), upright: bool) -> f32 {
    if upright {
        f32::from(high) / f32::from(wide)
    } else {
        f32::from(wide) / f32::from(high)
    }
}

/// The part of a camera's input the device sees. The page crops its frames
/// by it, and tetherd crops the frames of its own feed.
#[derive(Clone, Copy, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Framing {
    /// The factor by which the part is smaller than the largest part of that
    /// shape that fits: 1 for the largest, and at most [`Framing::CLOSEST`].
    pub zoom: f32,
    /// The centre of the part, from 0 at the input's left and top to 1 at
    /// its right and bottom. The part is clamped inside the input.
    pub x: f32,
    pub y: f32,
    /// The clockwise quarter turns applied to the input first, from 0 to 3,
    /// and whether it is mirrored left to right before that. The part is
    /// taken from the input after these.
    pub quarters: u8,
    pub mirrored: bool,
}

impl Framing {
    pub const CLOSEST: f32 = 8.0;

    /// The part that is seen: its left, top, width and height, each as a
    /// fraction of the turned input. `given` is that input's aspect ratio and
    /// `camera` the device view's, both width over height.
    pub fn seen(self, given: f32, camera: f32) -> (f32, f32, f32, f32) {
        let zoom = self.zoom.clamp(1.0, Self::CLOSEST);
        // The largest part that fits spans the input's full height if the
        // input is wider than the view, and its full width otherwise.
        let (width, height) = if given > camera {
            (camera / given / zoom, 1.0 / zoom)
        } else {
            (1.0 / zoom, given / camera / zoom)
        };

        (
            (self.x - width / 2.0).clamp(0.0, 1.0 - width),
            (self.y - height / 2.0).clamp(0.0, 1.0 - height),
            width,
            height,
        )
    }
}

impl Default for Framing {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            x: 0.5,
            y: 0.5,
            quarters: 0,
            mirrored: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_a_part_of_the_shape_of_the_camera() {
        let whole = Framing::default();

        assert_eq!(whole.seen(1.5, 1.5), (0.0, 0.0, 1.0, 1.0));
        assert_eq!(whole.seen(3.0, 1.5), (0.25, 0.0, 0.5, 1.0));
        assert_eq!(whole.seen(0.75, 1.5), (0.0, 0.25, 1.0, 0.5));

        let corner = Framing {
            zoom: 2.0,
            x: 1.0,
            y: 0.0,
            ..whole
        };

        assert_eq!(corner.seen(1.5, 1.5), (0.5, 0.0, 0.5, 0.5));
    }
}
