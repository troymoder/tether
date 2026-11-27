use android::icon::{Color, Resource};

use super::image_kind;

/// Looks up `reference`, written `@0x7f0801f0` as a file refers to it. The
/// dump lists it as `resource 0x7f0801f0 drawable/name`, followed by one line
/// per configuration: `(xhdpi) (file) res/pt.png type=PNG` or `() #ff9c27b0`.
pub(super) fn resource(resources: &str, reference: &str) -> Option<Resource> {
    let id = reference.strip_prefix('@')?;
    let forms = resources
        .lines()
        .map(str::trim)
        .skip_while(|line| !line.starts_with(&format!("resource {id} ")))
        .skip(1)
        .take_while(|line| line.starts_with('('));
    let mut image = None;
    let mut xml = None;

    for form in forms {
        let mut words = form.split(' ').skip(1);

        match words.next()? {
            // The densest image is listed last, so later ones win.
            "(file)" => match words.next() {
                Some(path) if image_kind(path).is_some() => image = Some(path),
                Some(path) if path.ends_with(".xml") => xml = Some(path),
                _ => {}
            },
            color => {
                if let Some(color) = Color::parse(color) {
                    return Some(Resource::Color(color));
                }
            }
        }
    }

    image
        .map(|path| Resource::Image(path.to_owned()))
        .or(xml.map(|path| Resource::Xml(path.to_owned())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_what_a_resource_is() {
        let resources = "    resource 0x7f06006c color/ic_launcher_background\n      \
            () #ff9c27b0\n    \
            resource 0x7f0801f1 drawable/ic_launcher_foreground\n      \
            (mdpi) (file) res/bB.png type=PNG\n      \
            (xhdpi) (file) res/tP.png type=PNG\n    \
            resource 0x7f0801f2 drawable/drawn\n      \
            () (file) res/Qr.xml type=XML\n";

        assert_eq!(
            resource(resources, "@0x7f06006c"),
            Some(Resource::Color(Color("#9c27b0".to_owned(), 1.0)))
        );
        assert_eq!(
            resource(resources, "@0x7f0801f1"),
            Some(Resource::Image("res/tP.png".to_owned()))
        );
        assert_eq!(
            resource(resources, "@0x7f0801f2"),
            Some(Resource::Xml("res/Qr.xml".to_owned()))
        );
        assert_eq!(resource(resources, "@0x7f0801ff"), None);
    }
}
