#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Source {
    Off,
    File,
    /// A camera of this browser.
    Camera,
    Screen,
    /// A webcam on tetherd's machine, which tetherd frames itself.
    Webcam,
    /// A video or picture on tetherd's machine, which tetherd plays and frames
    /// itself.
    RemoteFile,
}

impl Source {
    /// True for what this browser sends itself. Only these can be sent
    /// lossless, since the rest never passes through here.
    pub(super) fn local(self) -> bool {
        matches!(self, Source::File | Source::Camera | Source::Screen)
    }

    pub(super) fn remote(self) -> bool {
        matches!(self, Source::Webcam | Source::RemoteFile)
    }

    pub(super) fn icon(self) -> &'static str {
        match self {
            Source::Off => "M4 4l16 16M9 5h6l1 2h4v10M4 7v12h12",
            Source::File | Source::RemoteFile => "M6 3h8l4 4v14H6zM14 3v4h4",
            Source::Camera | Source::Webcam => "M3 7h12v10H3zM15 10l6-3v10l-6-3",
            Source::Screen => "M3 4h18v12H3zM8 20h8M12 16v4",
        }
    }

    pub(super) fn kind(self) -> Option<&'static str> {
        match self {
            Source::Off => None,
            Source::File => Some("File on this computer"),
            Source::Camera => Some("Webcam on this computer"),
            Source::Screen => Some("Screen of this computer"),
            Source::Webcam => Some("Webcam on tetherd"),
            Source::RemoteFile => Some("File on tetherd"),
        }
    }
}

/// The size and rate a webcam is asked for to feed `camera`. The size is
/// `size` if the webcam has it, else the smallest at least as large as the
/// camera, else the largest. The rate is `rate` if that size has it, else the
/// camera's rate or the nearest below, else the slowest.
pub(super) fn fitting(
    webcam: &protocol::Webcam,
    size: Option<(u16, u16)>,
    rate: Option<u16>,
    camera: &protocol::CameraSettings,
) -> Option<((u16, u16), u16)> {
    let chosen = webcam
        .sizes
        .iter()
        .find(|known| Some((known.width, known.height)) == size)
        .or_else(|| {
            webcam
                .sizes
                .iter()
                .rfind(|known| known.width >= camera.width && known.height >= camera.height)
        })
        .or_else(|| webcam.sizes.first())?;
    let rate = rate
        .filter(|rate| chosen.rates.contains(rate))
        .or_else(|| {
            chosen
                .rates
                .iter()
                .copied()
                .find(|rate| *rate <= u16::from(camera.fps))
        })
        .or_else(|| chosen.rates.last().copied())?;

    Some(((chosen.width, chosen.height), rate))
}

#[cfg(test)]
mod tests {
    use protocol::{CameraSettings, Webcam, WebcamSize};

    use super::*;

    fn webcam() -> Webcam {
        let size = |width, height, rates: &[u16]| WebcamSize {
            width,
            height,
            rates: rates.to_vec(),
            format: "mjpeg".to_owned(),
        };

        Webcam {
            path: "/dev/video0".to_owned(),
            name: "Webcam".to_owned(),
            sizes: vec![
                size(1920, 1080, &[60, 30]),
                size(1280, 720, &[30, 15]),
                size(640, 480, &[30]),
            ],
        }
    }

    const CAMERA: CameraSettings = CameraSettings {
        width: 1280,
        height: 720,
        fps: 30,
    };

    #[test]
    fn a_webcam_gives_what_was_asked_where_it_can() {
        assert_eq!(
            fitting(&webcam(), Some((1920, 1080)), Some(60), &CAMERA),
            Some(((1920, 1080), 60))
        );

        assert_eq!(
            fitting(&webcam(), Some((1920, 1080)), Some(25), &CAMERA),
            Some(((1920, 1080), 30))
        );
    }

    #[test]
    fn a_webcam_otherwise_fits_the_camera() {
        assert_eq!(
            fitting(&webcam(), None, None, &CAMERA),
            Some(((1280, 720), 30))
        );

        let large = CameraSettings {
            width: 4000,
            height: 3000,
            fps: 10,
        };

        assert_eq!(
            fitting(&webcam(), None, None, &large),
            Some(((1920, 1080), 30))
        );
    }
}
