pub(super) fn on_earth(latitude: f64, longitude: f64) -> bool {
    (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude)
}

/// The points of a route, as latitude and longitude, in a GPX or a KML
/// file. GPX has each as `<trkpt lat="…" lon="…">`, as `<rtept …>` in a
/// planned route, or as `<wpt …>` in a file of loose places. The first of
/// those kinds that a file has is used. KML has them between
/// `<coordinates>` as `longitude,latitude,height` with spaces between.
pub(super) fn points(file: &str) -> Vec<(f64, f64)> {
    let valid = |(latitude, longitude): &(f64, f64)| on_earth(*latitude, *longitude);

    if file.contains("<coordinates") {
        return file
            .split("<coordinates")
            .skip(1)
            .filter_map(|rest| rest.split_once('>')?.1.split("</coordinates").next())
            .flat_map(str::split_whitespace)
            .filter_map(|point| {
                let mut parts = point.split(',');
                let longitude = parts.next()?.parse().ok()?;

                Some((parts.next()?.parse().ok()?, longitude))
            })
            .filter(valid)
            .collect();
    }

    ["<trkpt", "<rtept", "<wpt"]
        .into_iter()
        .map(|kind| {
            file.split(kind)
                .skip(1)
                // Not a tag whose name only starts the same.
                .filter(|rest| rest.starts_with(char::is_whitespace))
                .filter_map(|rest| {
                    let tag = rest.split('>').next()?;

                    Some((attribute(tag, "lat")?, attribute(tag, "lon")?))
                })
                .filter(valid)
                .collect::<Vec<_>>()
        })
        .find(|points| !points.is_empty())
        .unwrap_or_default()
}

fn attribute(tag: &str, name: &str) -> Option<f64> {
    let rest = tag.split_once(&format!("{name}="))?.1;
    let quote = rest
        .chars()
        .next()
        .filter(|quote| ['"', '\''].contains(quote))?;

    rest[1..].split(quote).next()?.trim().parse::<f64>().ok()
}

/// The distance from one point to another over the ground, in metres, by
/// the haversine formula on a sphere of the earth's mean radius.
fn distance(from: (f64, f64), to: (f64, f64)) -> f64 {
    let from_latitude = from.0.to_radians();
    let to_latitude = to.0.to_radians();
    let across = (to.0 - from.0).to_radians() / 2.0;
    let along = (to.1 - from.1).to_radians() / 2.0;
    let half = across.sin().powi(2) + from_latitude.cos() * to_latitude.cos() * along.sin().powi(2);

    6_371_000.0 * 2.0 * half.sqrt().asin()
}

/// The length of a route, in metres.
pub(super) fn length(route: &[(f64, f64)]) -> f64 {
    route
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum()
}

/// The point `gone` metres along a route.
pub(super) fn along(route: &[(f64, f64)], mut gone: f64) -> (f64, f64) {
    for pair in route.windows(2) {
        let length = distance(pair[0], pair[1]);

        if gone <= length && length > 0.0 {
            let part = gone / length;

            return (
                pair[0].0 + (pair[1].0 - pair[0].0) * part,
                pair[0].1 + (pair[1].1 - pair[0].1) * part,
            );
        }

        gone -= length;
    }

    route.last().copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_points_of_a_route() {
        let gpx = r#"<gpx><wpt lat="1" lon="2"/><trk><trkseg>
            <trkpt lat="-33.9249" lon="18.4241"><ele>12</ele></trkpt>
            <trkpt lon='18.4300' lat='-33.9200'/>
            <trkpt lat="999" lon="0"/>
            </trkseg></trk></gpx>"#;

        assert_eq!(points(gpx), [(-33.9249, 18.4241), (-33.92, 18.43)]);

        assert_eq!(
            points(r#"<rte><rtept lat="1.5" lon="2.5"/></rte>"#),
            [(1.5, 2.5)]
        );

        assert_eq!(points(r#"<wpt lat="1" lon="2"/>"#), [(1.0, 2.0)]);

        let kml = "<LineString><coordinates>\n 18.4241,-33.9249,0 18.43,-33.92\n\
            </coordinates></LineString>";

        assert_eq!(points(kml), [(-33.9249, 18.4241), (-33.92, 18.43)]);

        assert!(points("not a route").is_empty());
    }

    #[test]
    fn goes_along_a_route() {
        // A degree of latitude is about 111 km.
        let route = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)];
        let first = distance(route[0], route[1]);

        assert!((first - 111_195.0).abs() < 100.0, "{first}");
        assert!((length(&route) - first - distance(route[1], route[2])).abs() < 1e-6);

        assert_eq!(along(&route, 0.0), (0.0, 0.0));

        let (latitude, longitude) = along(&route, first / 2.0);

        assert!((latitude - 0.5).abs() < 1e-9 && longitude == 0.0);

        let (latitude, longitude) = along(&route, first + distance(route[1], route[2]) / 4.0);

        assert!(latitude == 1.0 && (longitude - 0.25).abs() < 1e-9);

        assert_eq!(along(&route, 1e9), (1.0, 1.0));
    }
}
