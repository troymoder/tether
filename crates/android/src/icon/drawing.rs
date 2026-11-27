use std::collections::HashMap;

use html_escape::encode_double_quoted_attribute;

use super::{Color, Element, Package, Resource};

pub(super) struct Drawing<'a, P> {
    package: &'a P,
    /// Keyed by the reference the drawing uses for each file.
    files: &'a HashMap<String, Element>,
    /// SVG keeps gradient definitions in `<defs>`, apart from the shapes they
    /// fill.
    gradients: String,
    named: u32,
}

impl<'a, P: Package> Drawing<'a, P> {
    /// Ids start after `first`, so two drawings in one SVG do not clash.
    pub(super) fn new(package: &'a P, files: &'a HashMap<String, Element>, first: u32) -> Self {
        Self {
            package,
            files,
            gradients: String::new(),
            named: first,
        }
    }

    fn color(&self, value: &str) -> Option<Color> {
        Color::parse(value).or_else(|| match self.package.resource(value)? {
            Resource::Color(color) => Some(color),
            _ => None,
        })
    }

    fn paint(&mut self, value: &str) -> (String, f32) {
        if let Some(Color(color, opacity)) = self.color(value) {
            return (color, opacity);
        }

        // shortcut: colours from the device's theme or from the framework are
        // not resolved and come out grey. Resolve them if icons look wrong.
        let unknown = ("#888888".to_owned(), 1.0);

        let Some(file) = self.files.get(value) else {
            return unknown;
        };

        match file.name.as_str() {
            "gradient" => self.gradient(file).unwrap_or(unknown),
            // A colour state list. Its last item is the default state.
            "selector" => file
                .children
                .last()
                .and_then(|item| self.color(item.get("color")?))
                .map_or(unknown, |Color(color, opacity)| (color, opacity)),
            _ => unknown,
        }
    }

    fn gradient(&mut self, gradient: &Element) -> Option<(String, f32)> {
        let at = |name: &str| gradient.number(name).unwrap_or(0.0);
        let stops = self.stops(gradient);
        let (kind, place) = match gradient.number("type").unwrap_or(0.0) as u32 {
            0 => (
                "linearGradient",
                format!(
                    r#"x1="{}" y1="{}" x2="{}" y2="{}""#,
                    at("startX"),
                    at("startY"),
                    at("endX"),
                    at("endY")
                ),
            ),
            1 => (
                "radialGradient",
                format!(
                    r#"cx="{}" cy="{}" r="{}""#,
                    at("centerX"),
                    at("centerY"),
                    at("gradientRadius")
                ),
            ),
            // shortcut: SVG has no sweep gradient, so one is drawn in its
            // first colour. Approximate it with a conic pattern if needed.
            _ => {
                let (_, Color(color, opacity)) = stops.into_iter().next()?;

                return Some((color, opacity));
            }
        };

        self.named += 1;
        let name = format!("g{}", self.named);
        self.gradients.push_str(&format!(
            r#"<{kind} id="{name}" gradientUnits="userSpaceOnUse" {place}>"#
        ));

        for (offset, Color(color, opacity)) in stops {
            self.gradients.push_str(&format!(
                r#"<stop offset="{offset}" stop-color="{color}" stop-opacity="{opacity}"/>"#
            ));
        }

        self.gradients.push_str(&format!("</{kind}>"));

        Some((format!("url(#{name})"), 1.0))
    }

    fn stops(&self, gradient: &Element) -> Vec<(f32, Color)> {
        let items: Vec<(f32, Color)> = gradient
            .children
            .iter()
            .filter_map(|item| Some((item.number("offset")?, self.color(item.get("color")?)?)))
            .collect();

        if !items.is_empty() {
            return items;
        }

        [("startColor", 0.0), ("centerColor", 0.5), ("endColor", 1.0)]
            .into_iter()
            .filter_map(|(name, offset)| Some((offset, self.color(gradient.get(name)?)?)))
            .collect()
    }

    fn parts(&mut self, parts: &[Element]) -> String {
        let mut svg = String::new();
        // A clip path applies to the siblings after it until the group ends,
        // so each one opens an SVG group that is closed at the end.
        let mut cut = 0;

        for part in parts {
            match part.name.as_str() {
                "path" => svg.push_str(&self.path(part)),
                "group" => svg.push_str(&self.group(part)),
                "clip-path" => {
                    self.named += 1;
                    cut += 1;
                    svg.push_str(&format!(
                        r#"<clipPath id="c{0}"><path d="{1}"/></clipPath><g clip-path="url(#c{0})">"#,
                        self.named,
                        encode_double_quoted_attribute(part.get("pathData").unwrap_or_default())
                    ));
                }
                _ => {}
            }
        }

        svg.push_str(&"</g>".repeat(cut));

        svg
    }

    fn group(&mut self, group: &Element) -> String {
        let number = |name: &str, otherwise: f32| group.number(name).unwrap_or(otherwise);
        let x = number("pivotX", 0.0);
        let y = number("pivotY", 0.0);

        // Android scales, then rotates, both about the pivot, and then
        // translates.
        format!(
            r#"<g transform="translate({} {}) translate({x} {y}) rotate({}) scale({} {}) translate({} {})">{}</g>"#,
            number("translateX", 0.0),
            number("translateY", 0.0),
            number("rotation", 0.0),
            number("scaleX", 1.0),
            number("scaleY", 1.0),
            -x,
            -y,
            self.parts(&group.children)
        )
    }

    fn path(&mut self, path: &Element) -> String {
        let mut svg = format!(
            r#"<path d="{}""#,
            encode_double_quoted_attribute(path.get("pathData").unwrap_or_default())
        );

        // Android leaves a path without a fill colour unfilled.
        match path.get("fillColor") {
            Some(fill) => {
                let (fill, opacity) = self.paint(fill);
                let opacity = opacity * path.number("fillAlpha").unwrap_or(1.0);
                svg.push_str(&format!(r#" fill="{fill}" fill-opacity="{opacity}""#));

                if path.number("fillType") == Some(1.0) {
                    svg.push_str(r#" fill-rule="evenodd""#);
                }
            }
            None => svg.push_str(r#" fill="none""#),
        }

        if let Some(stroke) = path.get("strokeColor") {
            let (stroke, opacity) = self.paint(stroke);
            let opacity = opacity * path.number("strokeAlpha").unwrap_or(1.0);
            let end = ["butt", "round", "square"];
            let corner = ["miter", "round", "bevel"];
            let of = |names: [&'static str; 3], name: &str| {
                names[(path.number(name).unwrap_or(0.0) as usize).min(2)]
            };

            svg.push_str(&format!(
                r#" stroke="{stroke}" stroke-opacity="{opacity}" stroke-width="{}" stroke-linecap="{}" stroke-linejoin="{}" stroke-miterlimit="{}""#,
                path.number("strokeWidth").unwrap_or(0.0),
                of(end, "strokeLineCap"),
                of(corner, "strokeLineJoin"),
                path.number("strokeMiterLimit").unwrap_or(4.0)
            ));
        }

        svg.push_str("/>");

        svg
    }

    pub(super) fn svg(mut self, vector: &Element, size: Option<f32>) -> String {
        let width = vector.number("viewportWidth").unwrap_or(24.0);
        let height = vector.number("viewportHeight").unwrap_or(24.0);
        let (wide, high) = size.map_or((width, height), |size| (size, size));
        let parts = self.parts(&vector.children);

        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{wide}" height="{high}" viewBox="0 0 {width} {height}" opacity="{}"><defs>{}</defs>{parts}</svg>"#,
            vector.number("alpha").unwrap_or(1.0),
            self.gradients
        )
    }
}

pub(super) fn references(element: &Element, found: &mut Vec<String>) {
    for (name, value) in &element.attributes {
        if matches!(name.as_str(), "fillColor" | "strokeColor") && value.starts_with('@') {
            found.push(value.clone());
        }
    }

    for child in &element.children {
        references(child, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANDROID: &str = "http://schemas.android.com/apk/res/android";

    struct Colors;

    impl Package for Colors {
        fn resource(&self, reference: &str) -> Option<Resource> {
            let color = Color::parse("#ffff8800")?;

            (reference == "@0x7f060375").then_some(Resource::Color(color))
        }

        async fn xml(&self, path: &str) -> anyhow::Result<Element> {
            anyhow::bail!("no {path}")
        }

        async fn file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
            anyhow::bail!("no {path}")
        }
    }

    #[test]
    fn turns_a_drawing_into_an_svg() {
        let tree = format!(
            "N: android={ANDROID} (line=6)\n  \
             E: vector (line=6)\n    \
             A: {ANDROID}:viewportWidth(0x01010402)=108\n    \
             A: {ANDROID}:viewportHeight(0x01010403)=108\n      \
             E: path (line=7)\n        \
             A: {ANDROID}:fillColor(0x01010404)=@0x7f080028\n        \
             A: {ANDROID}:pathData(0x01010405)=\"M0,0h108v108z\" (Raw: \"M0,0h108v108z\")\n      \
             E: group (line=9)\n        \
             A: {ANDROID}:translateX(0x0101045a)=4.5\n          \
             E: clip-path (line=10)\n            \
             A: {ANDROID}:pathData(0x01010405)=\"M0,0h9v9z\" (Raw: \"M0,0h9v9z\")\n          \
             E: path (line=11)\n            \
             A: {ANDROID}:pathData(0x01010405)=\"M1,1h2\" (Raw: \"M1,1h2\")\n            \
             A: {ANDROID}:strokeColor(0x01010406)=#80ffffff\n            \
             A: {ANDROID}:strokeWidth(0x01010407)=2\n            \
             A: {ANDROID}:strokeLineCap(0x0101040b)=1\n"
        );
        let gradient = format!(
            "E: gradient (line=15)\n  \
             A: {ANDROID}:type(0x010101a1)=0\n  \
             A: {ANDROID}:startX(0x01010510)=54\n  \
             A: {ANDROID}:startY(0x01010511)=0.000000e+00\n  \
             A: {ANDROID}:endX(0x01010512)=54\n  \
             A: {ANDROID}:endY(0x01010513)=108\n    \
             E: item (line=19)\n      \
             A: {ANDROID}:color(0x010101a5)=@0x7f060375\n      \
             A: {ANDROID}:offset(0x01010514)=0.000000e+00\n    \
             E: item (line=22)\n      \
             A: {ANDROID}:color(0x010101a5)=#00000000\n      \
             A: {ANDROID}:offset(0x01010514)=1\n"
        );
        let resources = Colors;

        let vector = Element::parse(&tree).unwrap();
        assert_eq!(vector.name, "vector");
        assert_eq!(vector.children.len(), 2);
        assert_eq!(
            vector.children[1].children[1].get("pathData"),
            Some("M1,1h2")
        );

        let mut wanted = Vec::new();
        references(&vector, &mut wanted);
        assert_eq!(wanted, ["@0x7f080028"]);

        let files = HashMap::from([(wanted.remove(0), Element::parse(&gradient).unwrap())]);
        let drawing = Drawing::new(&resources, &files, 0);

        assert_eq!(
            drawing.svg(&vector, None),
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="108" height="108" viewBox="0 0 108 108" opacity="1">"#,
                r#"<defs><linearGradient id="g1" gradientUnits="userSpaceOnUse" x1="54" y1="0" x2="54" y2="108">"#,
                r##"<stop offset="0" stop-color="#ff8800" stop-opacity="1"/>"##,
                r##"<stop offset="1" stop-color="#000000" stop-opacity="0"/></linearGradient></defs>"##,
                r#"<path d="M0,0h108v108z" fill="url(#g1)" fill-opacity="1"/>"#,
                r#"<g transform="translate(4.5 0) translate(0 0) rotate(0) scale(1 1) translate(-0 -0)">"#,
                r#"<clipPath id="c2"><path d="M0,0h9v9z"/></clipPath><g clip-path="url(#c2)">"#,
                r##"<path d="M1,1h2" fill="none" stroke="#ffffff" stroke-opacity="0.5019608" stroke-width="2""##,
                r#" stroke-linecap="round" stroke-linejoin="miter" stroke-miterlimit="4"/></g></g></svg>"#
            )
        );
    }
}
