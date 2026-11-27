use android::scrcpy::Camera;
use protocol::CameraMode;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Shown {
    pub(super) camera: Camera,
    /// None for a camera that lists no sizes, which scrcpy then opens as it
    /// sees fit.
    pub(super) mode: Option<CameraMode>,
}

pub(super) fn size_name(size: &str) -> String {
    size.replace('x', " × ")
}

pub(super) fn area(size: &str) -> Option<u64> {
    let (width, height) = size.split_once('x')?;

    Some(width.parse::<u64>().ok()? * height.parse::<u64>().ok()?)
}

pub(super) fn aspect(size: &str) -> Option<String> {
    let (width, height) = size.split_once('x')?;
    let ratio = width.parse::<f32>().ok()? / height.parse::<f32>().ok()?;
    let usual = [
        ("4:3", 4.0 / 3.0),
        ("16:9", 16.0 / 9.0),
        ("1:1", 1.0),
        ("2:1", 2.0),
        ("3:2", 1.5),
        ("11:9", 11.0 / 9.0),
    ]
    .iter()
    .find(|(_, near)| (ratio - near).abs() < 0.02);

    Some(usual.map_or_else(|| format!("{ratio:.2}:1"), |(name, _)| (*name).to_owned()))
}

pub(super) fn camera_name(camera: &Camera) -> String {
    let mut name = camera.facing.to_string();

    if let Some(first) = name.get_mut(..1) {
        first.make_ascii_uppercase();
    }

    name
}

pub(super) fn rates_at(camera: &Camera, size: &str) -> Vec<u32> {
    let fast = camera
        .fast
        .iter()
        .find(|(one, _)| one == size)
        .map(|(_, rates)| rates.clone())
        .unwrap_or_default();

    camera.rates.iter().copied().chain(fast).collect()
}

pub(super) fn key(what: &str, device: &str, camera: &Camera) -> String {
    format!("camera-{what}-{device}-{}", camera.id)
}

pub(super) fn mode_text(mode: &CameraMode) -> String {
    format!("{}@{}", mode.size, mode.rate)
}

fn kept_mode(camera: &Camera, kept: &str) -> Option<CameraMode> {
    let (size, rate) = kept.split_once('@')?;
    let rate = rate.parse().ok()?;

    (camera.sizes.iter().any(|one| one == size) && rates_at(camera, size).contains(&rate)).then(
        || CameraMode {
            size: size.to_owned(),
            rate,
        },
    )
}

/// `sizes` is largest first, so the fallback is the largest size.
fn usual_mode(camera: &Camera) -> Option<CameraMode> {
    let size = camera
        .sizes
        .iter()
        .find(|one| *one == "1920x1080")
        .or_else(|| camera.sizes.first())?;
    let rate = if camera.rates.contains(&30) {
        30
    } else {
        camera.rates.first().copied().unwrap_or(30)
    };

    Some(CameraMode {
        size: size.clone(),
        rate,
    })
}

pub(super) fn shown(camera: Camera, kept: Option<&str>) -> Shown {
    let mode = kept
        .and_then(|kept| kept_mode(&camera, kept))
        .or_else(|| usual_mode(&camera));

    Shown { camera, mode }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        Camera {
            id: "0".to_owned(),
            facing: android::scrcpy::Facing::Back,
            rates: vec![15, 30],
            sizes: vec!["4000x3000".to_owned(), "1920x1080".to_owned()],
            fast: vec![("1920x1080".to_owned(), vec![120])],
            ..Camera::default()
        }
    }

    fn mode(size: &str, rate: u32) -> Option<CameraMode> {
        Some(CameraMode {
            size: size.to_owned(),
            rate,
        })
    }

    #[test]
    fn a_size_reads_as_its_shape() {
        assert_eq!(size_name("1920x1080"), "1920 × 1080");
        assert_eq!(area("1920x1080"), Some(1920 * 1080));
        assert_eq!(area("junk"), None);
        assert_eq!(aspect("1920x1080").as_deref(), Some("16:9"));
        assert_eq!(aspect("640x480").as_deref(), Some("4:3"));
        assert_eq!(aspect("1850x1000").as_deref(), Some("1.85:1"));
        assert_eq!(aspect("x"), None);
    }

    #[test]
    fn a_camera_opens_as_it_was_left_or_as_usual() {
        assert_eq!(shown(camera(), None).mode, mode("1920x1080", 30));
        assert_eq!(
            shown(camera(), Some("4000x3000@15")).mode,
            mode("4000x3000", 15)
        );
        assert_eq!(
            shown(camera(), Some("1920x1080@120")).mode,
            mode("1920x1080", 120)
        );
        // A size or a rate the camera does not have is not kept to.
        assert_eq!(
            shown(camera(), Some("4000x3000@120")).mode,
            mode("1920x1080", 30)
        );
        assert_eq!(shown(camera(), Some("junk")).mode, mode("1920x1080", 30));
        assert_eq!(shown(Camera::default(), Some("1x1@30")).mode, None);
    }

    #[test]
    fn a_mode_is_kept_as_text() {
        let kept = mode_text(&CameraMode {
            size: "640x480".to_owned(),
            rate: 60,
        });

        assert_eq!(kept, "640x480@60");
    }
}
