//! An icon is an image, a vector drawable, or an adaptive icon. An adaptive
//! icon has a background and a foreground layer, each an image, a colour or a
//! vector drawable, and only their middle shows.

mod drawing;
mod xml;

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub use xml::Element;

use self::drawing::{Drawing, references};

/// The side length of an adaptive icon's layers, of which the middle two
/// thirds show.
const LAYER: f32 = 108.0;

pub fn image_kind(path: &str) -> Option<&'static str> {
    match path.rsplit_once('.')?.1 {
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

/// A colour as SVG takes it: `#rrggbb` and a separate opacity.
#[derive(Clone, Debug, PartialEq)]
pub struct Color(pub String, pub f32);

impl Color {
    /// Reads Android's `#aarrggbb` or `#rrggbb`.
    pub fn parse(text: &str) -> Option<Color> {
        let digits = text.strip_prefix('#')?;

        if !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
            return None;
        }

        match digits.len() {
            6 => Some(Color(format!("#{digits}"), 1.0)),
            8 => {
                let opacity = u8::from_str_radix(&digits[..2], 16).ok()?;

                Some(Color(
                    format!("#{}", &digits[2..]),
                    f32::from(opacity) / 255.0,
                ))
            }
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Resource {
    Color(Color),
    Image(String),
    Xml(String),
}

pub trait Package {
    /// `reference` is written `@0x7f0801f0`, as XML files refer to it. Of its
    /// variants for each screen density, the densest image wins, then an XML
    /// file.
    fn resource(&self, reference: &str) -> Option<Resource>;

    fn xml(&self, path: &str) -> impl Future<Output = Result<Element>>;

    fn file(&self, path: &str) -> impl Future<Output = Result<Vec<u8>>>;
}

async fn image(package: &impl Package, path: &str, size: f32) -> Result<String> {
    let kind = image_kind(path).context("not an image")?;
    let bytes = package.file(path).await?;

    Ok(format!(
        r#"<image width="{size}" height="{size}" href="data:{kind};base64,{}"/>"#,
        STANDARD.encode(bytes)
    ))
}

async fn drawing(
    package: &impl Package,
    vector: &Element,
    size: Option<f32>,
    first: u32,
) -> String {
    let mut wanted = Vec::new();

    references(vector, &mut wanted);

    let mut files = HashMap::new();

    for reference in wanted {
        if let Some(Resource::Xml(path)) = package.resource(&reference)
            && let Ok(file) = package.xml(&path).await
        {
            files.insert(reference, file);
        }
    }

    Drawing::new(package, &files, first).svg(vector, size)
}

async fn layer(package: &impl Package, value: &str, first: u32) -> Result<String> {
    Ok(match package.resource(value) {
        Some(Resource::Color(Color(color, opacity))) => format!(
            r#"<rect width="{LAYER}" height="{LAYER}" fill="{color}" fill-opacity="{opacity}"/>"#
        ),
        Some(Resource::Image(path)) => image(package, &path, LAYER).await?,
        Some(Resource::Xml(path)) => {
            let file = package.xml(&path).await?;

            // shortcut: other layer drawables (a layer list, an inset image)
            // are refused. Support them if real icons use them.
            if file.name != "vector" {
                bail!("unsupported icon type: {}", file.name);
            }

            drawing(package, &file, Some(LAYER), first).await
        }
        None => bail!("icon layer missing from the APK"),
    })
}

pub async fn svg(package: &impl Package, icon: &str) -> Result<String> {
    if image_kind(icon).is_some() {
        return Ok(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1">{}</svg>"#,
            image(package, icon, 1.0).await?
        ));
    }

    let file = package.xml(icon).await?;

    match file.name.as_str() {
        "vector" => Ok(drawing(package, &file, None, 0).await),
        "adaptive-icon" => adaptive(package, &file).await,
        other => bail!("unsupported icon type: {other}"),
    }
}

async fn adaptive(package: &impl Package, file: &Element) -> Result<String> {
    let mut layers = String::new();

    // The background is drawn first. Each layer numbers its ids from its own
    // start so they do not clash.
    for (name, first) in [("background", 0), ("foreground", 1000)] {
        let value = file
            .children
            .iter()
            .find(|layer| layer.name == name)
            .and_then(|layer| layer.get("drawable"))
            .with_context(|| format!("icon has no {name}"))?;

        layers.push_str(&layer(package, value, first).await?);
    }

    let edge = LAYER / 6.0;
    let shown = LAYER * 2.0 / 3.0;

    Ok(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{edge} {edge} {shown} {shown}">{layers}</svg>"#
    ))
}
