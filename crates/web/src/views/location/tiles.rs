//! The tile sources share the Web Mercator projection. At zoom `z` the world
//! is `2^z` tiles across and down.

use std::f64::consts::PI;

/// The width and height of a tile in pixels.
pub(super) const TILE: f64 = 256.0;

pub(super) struct Kind {
    pub(super) name: &'static str,
    /// The tile address, with `{z}`, `{x}` and `{y}` filled in per tile.
    pub(super) tiles: &'static str,
    /// True for light tiles that the page inverts into a dark map.
    pub(super) darkened: bool,
    pub(super) credit: &'static str,
    pub(super) more: &'static str,
    pub(super) closest: u8,
}

pub(super) const KINDS: [Kind; 4] = [
    Kind {
        name: "Dark",
        tiles: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
        darkened: true,
        credit: "© OpenStreetMap contributors",
        more: "https://www.openstreetmap.org/copyright",
        closest: 19,
    },
    Kind {
        name: "Light",
        tiles: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
        darkened: false,
        credit: "© OpenStreetMap contributors",
        more: "https://www.openstreetmap.org/copyright",
        closest: 19,
    },
    Kind {
        name: "Satellite",
        tiles: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
        darkened: false,
        credit: "Imagery © Esri, Maxar, Earthstar Geographics",
        more: "https://www.esri.com/en-us/legal/terms/full-master-agreement",
        closest: 19,
    },
    Kind {
        name: "Terrain",
        tiles: "https://tile.opentopomap.org/{z}/{x}/{y}.png",
        darkened: false,
        credit: "© OpenTopoMap, © OpenStreetMap contributors",
        more: "https://opentopomap.org/about",
        closest: 17,
    },
];

/// A tile as its zoom, column, row, and the column it is fetched as. The
/// last one wraps, since columns start over round the world.
pub(super) type Tile = (u8, i64, i64, i64);

/// The width of the whole world in pixels at a zoom.
pub(super) fn across(zoom: u8) -> f64 {
    TILE * f64::from(1u32 << zoom)
}

/// The pixel position of a place on the whole world's map at a zoom,
/// measured from its top left corner.
pub(super) fn spot(latitude: f64, longitude: f64, zoom: u8) -> (f64, f64) {
    // Clamped short of the poles, where the projection goes to infinity.
    let sine = latitude.to_radians().sin().clamp(-0.9999, 0.9999);

    (
        (longitude + 180.0) / 360.0 * across(zoom),
        (0.5 - ((1.0 + sine) / (1.0 - sine)).ln() / (4.0 * PI)) * across(zoom),
    )
}

/// The inverse of [`spot`].
pub(super) fn place(x: f64, y: f64, zoom: u8) -> (f64, f64) {
    let up = PI - 2.0 * PI * y / across(zoom);
    let longitude = (x / across(zoom) * 360.0).rem_euclid(360.0) - 180.0;

    (up.sinh().atan().to_degrees().clamp(-85.0, 85.0), longitude)
}

pub(super) fn address(kind: usize, tile: Tile) -> String {
    let (zoom, _, y, wrapped) = tile;

    KINDS[kind]
        .tiles
        .replace("{z}", &zoom.to_string())
        .replace("{x}", &wrapped.to_string())
        .replace("{y}", &y.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_survives_the_projection() {
        let (x, y) = spot(52.37, 4.89, 15);
        let (latitude, longitude) = place(x, y, 15);

        assert!((latitude - 52.37).abs() < 1e-9);
        assert!((longitude - 4.89).abs() < 1e-9);
    }

    #[test]
    fn the_middle_of_the_world_is_null_island() {
        assert_eq!(spot(0.0, 0.0, 1), (256.0, 256.0));
    }

    #[test]
    fn a_tile_address_uses_the_wrapped_column() {
        assert_eq!(
            address(0, (3, 9, 2, 1)),
            "https://tile.openstreetmap.org/3/1/2.png"
        );
    }
}
