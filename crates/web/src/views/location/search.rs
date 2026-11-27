//! The browser asks Photon directly, which is built on OpenStreetMap and,
//! unlike OpenStreetMap's own search, allows a request per typed letter.
//! Photon receives the typed text, and the device's position to name the
//! place it stands at (standing.rs).

use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::route::on_earth;

const SEARCH: &str = "https://photon.komoot.io";

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Found {
    pub(super) name: Option<String>,
    pub(super) latitude: f64,
    pub(super) longitude: f64,
}

pub(super) fn around(latitude: f64, longitude: f64) -> String {
    format!("reverse?lat={latitude}&lon={longitude}")
}

pub(super) fn called_like(text: &str) -> String {
    format!("api/?limit=6&q={}", js_sys::encode_uri_component(text))
}

/// Reads typed text as a latitude and a longitude, separated by a comma or
/// a space as map apps give them.
pub(super) fn coordinates(text: &str) -> Option<(f64, f64)> {
    let mut numbers = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<f64>().ok());
    let latitude = numbers.next()??;
    let longitude = numbers.next()??;

    (numbers.next().is_none() && on_earth(latitude, longitude)).then_some((latitude, longitude))
}

fn called(place: &JsValue) -> Option<String> {
    let get = |key: &str| js_sys::Reflect::get(place, &key.into()).ok()?.as_string();
    let street = get("street").map(|street| match get("housenumber") {
        Some(number) => format!("{number} {street}"),
        None => street,
    });

    let mut parts = Vec::<String>::new();

    for part in [
        get("name"),
        street,
        get("city"),
        get("state"),
        get("country"),
    ]
    .into_iter()
    .flatten()
    {
        // A city is often in a state of the same name.
        if !parts.contains(&part) {
            parts.push(part);
        }
    }

    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Asks Photon for places: `asked` is what [`called_like`] or [`around`]
/// gives.
pub(super) async fn seek(asked: &str) -> Result<Vec<Found>, String> {
    let unreachable = |_| "search failed: not reachable".to_owned();

    let answer = JsFuture::from(window().fetch_with_str(&format!("{SEARCH}/{asked}")))
        .await
        .map_err(unreachable)?;
    let answer: web_sys::Response = answer.unchecked_into();
    let found = JsFuture::from(answer.json().map_err(unreachable)?)
        .await
        .map_err(unreachable)?;
    let get = |from: &JsValue, key: &str| js_sys::Reflect::get(from, &key.into()).ok();

    // Each result is a GeoJSON feature, whose point has the longitude first.
    Ok(get(&found, "features")
        .map(|features| js_sys::Array::from(&features))
        .unwrap_or_default()
        .iter()
        .filter_map(|place| {
            let at = js_sys::Array::from(&get(&get(&place, "geometry")?, "coordinates")?);

            Some(Found {
                name: called(&get(&place, "properties")?),
                latitude: at.get(1).as_f64()?,
                longitude: at.get(0).as_f64()?,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::coordinates;

    #[test]
    fn reads_coordinates_as_maps_give_them() {
        assert_eq!(coordinates("52.37, 4.89"), Some((52.37, 4.89)));
        assert_eq!(coordinates("52.37 4.89"), Some((52.37, 4.89)));
        assert_eq!(coordinates("Amsterdam"), None);
        assert_eq!(coordinates("52.37, 4.89, 3"), None);
        assert_eq!(coordinates("95, 4.89"), None);
    }
}
