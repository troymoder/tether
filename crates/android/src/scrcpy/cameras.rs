use std::fmt::Debug;

use adb::Device;
use adb::libadb::Session;
use anyhow::Result;

use super::{JAR, VERSION};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facing {
    Back,
    Front,
    External,
    /// scrcpy lists a camera as `unknown` when Android names no side for it
    /// (`getCameraFacingName` in its LogUtils.java).
    #[default]
    Unknown,
}

impl Facing {
    fn of(word: &str) -> Self {
        match word {
            "back" => Facing::Back,
            "front" => Facing::Front,
            "external" => Facing::External,
            _ => Facing::Unknown,
        }
    }
}

impl std::fmt::Display for Facing {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(match self {
            Facing::Back => "back",
            Facing::Front => "front",
            Facing::External => "external",
            Facing::Unknown => "unknown",
        })
    }
}

/// Sizes are written like `4000x3000`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Camera {
    pub id: String,
    pub facing: Facing,
    /// The sensor's size.
    pub size: String,
    /// The frame rates it supports at every size.
    pub rates: Vec<u32>,
    /// The zoom range from widest to closest, if the camera reports one. Below 1 is wider.
    pub zoom: Option<(f32, f32)>,
    /// Largest first.
    pub sizes: Vec<String>,
    /// The sizes it also supports in high-speed capture, with their frame rates.
    pub fast: Vec<(String, Vec<u32>)>,
}

/// scrcpy's server prints the cameras and exits.
pub async fn cameras<S>(device: &Device<S>, jar: &[u8]) -> Result<Vec<Camera>>
where
    S: Session + Clone,
    S::IoError: Debug,
{
    device.push(jar, JAR).await?;

    let arguments = adb::command([
        "app_process".to_owned(),
        "/".to_owned(),
        "com.genymobile.scrcpy.Server".to_owned(),
        VERSION.to_owned(),
        "log_level=info".to_owned(),
        "list_camera_sizes=true".to_owned(),
    ])?;

    let command = format!("CLASSPATH={JAR} {arguments}");
    // The input is kept open until the server exits, because closing it can end the server early.
    let adb::Process {
        input: _input,
        mut output,
    } = device.spawn(&command, None).await?;

    let mut printed = String::new();

    while let Some(said) = output.next().await? {
        if let adb::Said::Stdout(bytes) | adb::Said::Stderr(bytes) = said {
            printed.push_str(&String::from_utf8_lossy(&bytes));
        }
    }

    Ok(cameras_in(&printed))
}

/// `list_camera_sizes` prints a line for each camera, such as
/// `--camera-id=0    (back, 4000x3000, fps={15, 30}, zoom-range=[0.6, 10.0])`,
/// then its sizes as `- 1920x1080`, and after `High speed capture` its
/// high-speed sizes as `- 1920x1080 (fps={120, 240})`.
fn cameras_in(printed: &str) -> Vec<Camera> {
    let mut cameras: Vec<Camera> = Vec::new();
    let mut fast = false;

    for line in printed.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("--camera-id=") {
            let Some((id, about)) = rest.split_once(char::is_whitespace) else {
                continue;
            };

            let about = about.trim().trim_start_matches('(').trim_end_matches(')');
            let mut parts = about.split(", ");

            cameras.push(Camera {
                id: id.to_owned(),
                facing: parts.next().map(Facing::of).unwrap_or_default(),
                size: parts.next().unwrap_or_default().to_owned(),
                rates: rates(about),
                zoom: about.split_once("zoom-range=[").and_then(|(_, range)| {
                    let (least, most) = range.split_once(']')?.0.split_once(", ")?;

                    Some((least.parse().ok()?, most.parse().ok()?))
                }),
                ..Camera::default()
            });

            fast = false;
        } else if line.starts_with("High speed capture") {
            fast = true;
        } else if let (Some(size), Some(camera)) = (line.strip_prefix("- "), cameras.last_mut()) {
            let size = size
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();

            if fast {
                camera.fast.push((size, rates(line)));
            } else {
                camera.sizes.push(size);
            }
        }
    }

    cameras
}

fn rates(text: &str) -> Vec<u32> {
    text.split_once("fps={")
        .and_then(|(_, rest)| rest.split_once('}'))
        .map(|(list, _)| {
            list.split(", ")
                .filter_map(|rate| rate.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_cameras() {
        let printed = "[server] INFO: List of cameras:\n    \
            --camera-id=0    (back, 4000x3000, fps={15, 30}, zoom-range=[0.6, 10.0])\n        \
            - 4000x3000\n        \
            - 1920x1080\n      \
            High speed capture (--camera-high-speed):\n        \
            - 1920x1080 (fps={120, 240})\n    \
            --camera-id=1    (front, 3264x2448, fps={15, 30})\n        \
            - 1280x720\n";

        assert_eq!(
            cameras_in(printed),
            [
                Camera {
                    id: "0".to_owned(),
                    facing: Facing::Back,
                    size: "4000x3000".to_owned(),
                    rates: vec![15, 30],
                    zoom: Some((0.6, 10.0)),
                    sizes: vec!["4000x3000".to_owned(), "1920x1080".to_owned()],
                    fast: vec![("1920x1080".to_owned(), vec![120, 240])],
                },
                Camera {
                    id: "1".to_owned(),
                    facing: Facing::Front,
                    size: "3264x2448".to_owned(),
                    rates: vec![15, 30],
                    zoom: None,
                    sizes: vec!["1280x720".to_owned()],
                    fast: Vec::new(),
                },
            ]
        );
    }
}
