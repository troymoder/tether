use super::style;
use crate::link::MINUTE;
use crate::util::size;

pub(super) fn rate(bytes: u32) -> String {
    format!("{}/s", size(bytes.into()))
}

pub(super) fn lasted(milliseconds: f64) -> String {
    let seconds = (milliseconds / 1000.0).max(0.0) as u64;

    match (seconds / 3600, seconds / 60 % 60, seconds % 60) {
        (0, 0, seconds) => format!("{seconds} s"),
        (0, minutes, seconds) => format!("{minutes} min {seconds} s"),
        (hours, minutes, _) => format!("{hours} h {minutes} min"),
    }
}

pub(super) fn trip(milliseconds: f64) -> String {
    format!("{} ms", milliseconds.round().max(1.0))
}

/// Down and up are scaled against the larger peak of the two, and the round
/// trip and the packet loss each against its own peak.
pub(super) fn lines(
    traffic: &[(u32, u32)],
    trips: &[Option<f64>],
    losses: &[Option<f64>],
) -> String {
    let most = traffic
        .iter()
        .map(|(down, up)| (*down).max(*up))
        .max()
        .unwrap_or(0)
        .max(1) as f32;
    let first = MINUTE - traffic.len().min(MINUTE);
    let line = |class: &str, pick: fn(&(u32, u32)) -> u32| {
        let points: Vec<String> = traffic
            .iter()
            .enumerate()
            .map(|(at, second)| {
                format!(
                    "{},{:.1}",
                    first + at,
                    27.0 - pick(second) as f32 / most * 26.0
                )
            })
            .collect();

        format!(
            r#"<polyline class="{class}" points="{}"/>"#,
            points.join(" ")
        )
    };

    // The round trip's scale is at least 20 ms because a local round trip of
    // 0 or 1 ms would otherwise swing the line from bottom to top. The loss
    // scale is at least 5 %, so a lone lost packet shows as a small bump.
    format!(
        r#"<svg role="img" aria-label="Transfer and latency over the last minute" viewBox="0 0 {width} 28" preserveAspectRatio="none"><path class="{}" d="M0 27.5H{width}"/>{}{}{}{}</svg>"#,
        style::baseline,
        measured(style::latency, trips, 20.0),
        measured(style::loss, losses, 0.05),
        line(style::up, |second| second.1),
        line(style::down, |second| second.0),
        width = MINUTE - 1,
    )
}

fn measured(class: &str, seconds: &[Option<f64>], least: f64) -> String {
    let most = seconds
        .iter()
        .flatten()
        .fold(least, |most, figure| most.max(*figure));
    let first = MINUTE - seconds.len().min(MINUTE);
    let points: Vec<String> = seconds
        .iter()
        .enumerate()
        .filter_map(|(at, figure)| {
            let figure = (*figure)?;

            Some(format!("{},{:.1}", first + at, 27.0 - figure / most * 26.0))
        })
        .collect();

    format!(
        r#"<polyline class="{class}" points="{}"/>"#,
        points.join(" ")
    )
}

pub(super) fn loss(share: f64) -> String {
    format!("{:.1} %", share * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_is_in_its_two_largest_units() {
        assert_eq!(lasted(-5.0), "0 s");
        assert_eq!(lasted(42_000.0), "42 s");
        assert_eq!(lasted(252_000.0), "4 min 12 s");
        assert_eq!(lasted(3_723_000.0), "1 h 2 min");
    }

    #[test]
    fn a_round_trip_is_at_least_a_millisecond() {
        assert_eq!(trip(0.2), "1 ms");
        assert_eq!(trip(12.6), "13 ms");
    }
}
