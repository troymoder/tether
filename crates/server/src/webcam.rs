use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use protocol::{Webcam, WebcamSize};
use tokio::process::{Child, ChildStdout, Command};
#[cfg(target_os = "linux")]
use v4l::{frameinterval::FrameIntervalEnum, framesize::FrameSizeEnum, video::Capture};

const PREVIEW: usize = 640;

// shortcut: macOS lists no webcams, ask AVFoundation (ffmpeg -f avfoundation
// -list_devices) when a Mac is to feed cameras from its webcams.
#[cfg(not(target_os = "linux"))]
pub(crate) fn list() -> Vec<Webcam> {
    Vec::new()
}

/// Listing asks each V4L2 device, so it is slow.
#[cfg(target_os = "linux")]
pub(crate) fn list() -> Vec<Webcam> {
    let mut webcams: Vec<Webcam> = v4l::context::enum_devices()
        .into_iter()
        .filter_map(|node| {
            let path = node.path().to_str()?.to_owned();
            let sizes = sizes(node.path());
            // Some webcams end their name with a colon.
            let name = node
                .name()
                .map(|name| name.trim().trim_end_matches(':').trim().to_owned());

            // A device without sizes captures nothing, such as a webcam's
            // second node for its metadata.
            (!sizes.is_empty()).then(|| Webcam {
                name: name
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| path.clone()),
                path,
                sizes,
            })
        })
        .collect();

    webcams.sort_by(|one, other| one.path.cmp(&other.path));

    webcams
}

/// Largest first, each in the first format that offers it.
#[cfg(target_os = "linux")]
fn sizes(path: &Path) -> Vec<WebcamSize> {
    let Ok(device) = v4l::Device::with_path(path) else {
        return Vec::new();
    };

    let Ok(formats) = device.enum_formats() else {
        return Vec::new();
    };

    let mut sizes: Vec<WebcamSize> = Vec::new();

    for format in formats {
        let Some(named) = ffmpeg_format(&format.fourcc.repr) else {
            continue;
        };

        for size in device.enum_framesizes(format.fourcc).unwrap_or_default() {
            let FrameSizeEnum::Discrete(size) = size.size else {
                continue;
            };

            let Ok(width) = u16::try_from(size.width) else {
                continue;
            };

            let Ok(height) = u16::try_from(size.height) else {
                continue;
            };

            let rates = rates(&device, format.fourcc, size.width, size.height);
            let known = sizes
                .iter()
                .any(|known| (known.width, known.height) == (width, height));

            if !rates.is_empty() && !known {
                sizes.push(WebcamSize {
                    width,
                    height,
                    rates,
                    format: named.to_owned(),
                });
            }
        }
    }

    sizes.sort_by_key(|size| std::cmp::Reverse(u32::from(size.width) * u32::from(size.height)));

    sizes
}

// MJPG matters most, since a webcam sends its large sizes quickly only as
// JPEG.
#[cfg(target_os = "linux")]
fn ffmpeg_format(fourcc: &[u8; 4]) -> Option<&'static str> {
    match fourcc {
        b"MJPG" => Some("mjpeg"),
        b"YUYV" => Some("yuyv422"),
        // Virtual cameras give these.
        b"YU12" => Some("yuv420p"),
        b"NV12" => Some("nv12"),
        _ => None,
    }
}

/// Fastest first.
#[cfg(target_os = "linux")]
fn rates(device: &v4l::Device, fourcc: v4l::FourCC, width: u32, height: u32) -> Vec<u16> {
    let mut rates: Vec<u16> = device
        .enum_frameintervals(fourcc, width, height)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interval| match interval.interval {
            FrameIntervalEnum::Discrete(time) if time.numerator > 0 => {
                u16::try_from(time.denominator / time.numerator).ok()
            }
            _ => None,
        })
        .collect();

    rates.sort_unstable_by(|one, other| other.cmp(one));
    rates.dedup();

    rates
}

/// Dropping it stops ffmpeg, which releases the webcam.
pub(crate) struct Capturing {
    pub output: ChildStdout,
    _ffmpeg: Child,
}

pub(crate) fn capture(ffmpeg: &Path, path: &str, size: &WebcamSize, fps: u16) -> Result<Capturing> {
    let mut command = Command::new(ffmpeg);

    command
        .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
        .args(["-f", "v4l2", "-input_format", &size.format])
        .args(["-video_size", &format!("{}x{}", size.width, size.height)])
        .args(["-framerate", &fps.to_string(), "-i", path]);

    spawn(ffmpeg, &mut command)
}

pub(crate) fn play(
    ffmpeg: &Path,
    file: &Path,
    size: (u16, u16),
    picture: bool,
) -> Result<Capturing> {
    let mut command = Command::new(ffmpeg);

    command.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);

    if picture {
        command.args(["-loop", "1", "-framerate", "15"]);
    } else {
        command.args(["-stream_loop", "-1"]);
    }

    command.arg("-re").arg("-i").arg(file).args([
        "-an",
        "-vf",
        &format!("scale={}:{}", size.0, size.1),
    ]);

    spawn(ffmpeg, &mut command)
}

fn spawn(ffmpeg: &Path, command: &mut Command) -> Result<Capturing> {
    let mut child = command
        // Writes each frame out whole at once, instead of when a buffer fills.
        .args(["-flush_packets", "1"])
        .args(["-f", "rawvideo", "-pix_fmt", "yuv420p", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("{}", ffmpeg.display()))?;
    let output = child.stdout.take().context("ffmpeg: no output")?;

    Ok(Capturing {
        output,
        _ffmpeg: child,
    })
}

/// The colours are stretched from a camera's limited range to JPEG's full
/// range.
pub(crate) fn preview(picture: &[u8], (width, height): (usize, usize)) -> Option<Vec<u8>> {
    let scale = (width.div_ceil(PREVIEW)).max(1);
    let small_width = width / scale / 2 * 2;
    let small_height = height / scale / 2 * 2;
    let light = width * height;
    let quarter = width * height / 4;

    if picture.len() != light + 2 * quarter || small_width == 0 || small_height == 0 {
        return None;
    }

    let mut points = Vec::with_capacity(small_width * small_height * 3);
    let full = |value: u8, low: f32, span: f32| {
        ((f32::from(value) - low) * 255.0 / span).clamp(0.0, 255.0) as u8
    };

    for y in 0..small_height {
        for x in 0..small_width {
            let from_x = x * scale;
            let from_y = y * scale;
            let colour = from_y / 2 * (width / 2) + from_x / 2;

            points.push(full(picture[from_y * width + from_x], 16.0, 219.0));
            points.push(full(picture[light + colour], 16.0, 224.0));
            points.push(full(picture[light + quarter + colour], 16.0, 224.0));
        }
    }

    let mut jpeg = Vec::new();

    jpeg_encoder::Encoder::new(&mut jpeg, 70)
        .encode(
            &points,
            u16::try_from(small_width).ok()?,
            u16::try_from(small_height).ok()?,
            jpeg_encoder::ColorType::Ycbcr,
        )
        .ok()?;

    Some(jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_a_picture_as_a_small_jpeg() {
        let width = 1280;
        let height = 720;
        let mut picture = vec![128; width * height * 3 / 2];

        picture[..width * height].fill(235);

        let jpeg = preview(&picture, (width, height)).unwrap();

        assert_eq!(&jpeg[..2], [0xff, 0xd8]);
        assert!(preview(&picture[1..], (width, height)).is_none());
    }
}
