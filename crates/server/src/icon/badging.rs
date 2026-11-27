use super::image_kind;

/// Badging has a line per screen density, such as
/// `application-icon-640:'res/icon.png'`. The densest image wins, then any
/// other file.
pub(super) fn listed(badging: &str) -> Option<&str> {
    let mut icons: Vec<(u32, &str)> = badging
        .lines()
        .filter_map(|line| {
            let (density, path) = line.strip_prefix("application-icon-")?.split_once(':')?;

            Some((density.parse().ok()?, path.trim().trim_matches('\'')))
        })
        .collect();
    icons.sort();

    let images: Vec<(u32, &str)> = icons
        .iter()
        .copied()
        .filter(|(_, path)| image_kind(path).is_some())
        .collect();

    images
        .iter()
        // Densities from 65000 up stand for "any density", which is no
        // sharper.
        .rfind(|(density, _)| *density < 65_000)
        .or(images.last())
        .or(icons.last())
        .map(|(_, path)| *path)
}

/// From `application-label:'Example'`.
pub(super) fn label(badging: &str) -> Option<String> {
    let label = badging
        .lines()
        .find_map(|line| line.strip_prefix("application-label:"))?;
    let label = label.trim().strip_prefix('\'')?.strip_suffix('\'')?;

    Some(label.replace("\\'", "'")).filter(|label| !label.is_empty())
}

/// From `package: name='com.example' versionCode='1'`.
pub(super) fn named(badging: &str) -> Option<&str> {
    let rest = badging
        .lines()
        .find_map(|line| line.strip_prefix("package: name='"))?;

    Some(rest.split_once('\'')?.0)
}

/// From the `package:` line's `versionName='2.4.0'`.
pub(super) fn version(badging: &str) -> Option<&str> {
    let package = badging.lines().find(|line| line.starts_with("package: "))?;
    let rest = package.split_once(" versionName='")?.1;

    Some(rest.split_once('\'')?.0).filter(|version| !version.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_sharpest_image() {
        let badging = "application-label:'Example'\n\
            application-icon-160:'res/a.png'\n\
            application-icon-640:'res/c.png'\n\
            application-icon-480:'res/b.png'\n\
            application-icon-65534:'res/any.png'\n\
            application: label='Example' icon='res/any.png'\n";

        assert_eq!(listed(badging), Some("res/c.png"));
        assert_eq!(label(badging).as_deref(), Some("Example"));
        assert_eq!(
            named("package: name='com.example' versionCode='1'\napplication-label:'A'\n"),
            Some("com.example")
        );
        assert_eq!(
            label("application-label:'It\\'s'\n").as_deref(),
            Some("It's")
        );
        assert_eq!(
            listed("application-icon-640:'res/BW.xml'\napplication-icon-65534:'res/BW.xml'\n"),
            Some("res/BW.xml")
        );
        assert_eq!(listed("application: label='Example'\n"), None);
        assert_eq!(
            version(
                "package: name='com.example' versionCode='213' versionName='2.4.0' platformBuildVersionName='16'\n"
            ),
            Some("2.4.0")
        );
        assert_eq!(
            version("package: name='com.example' versionCode='1'\n"),
            None
        );
    }
}
