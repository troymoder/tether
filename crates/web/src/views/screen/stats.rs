//! The video stats are counted all the time and read once a second while
//! their strip is open.

use std::time::Duration;

use leptos::prelude::*;

use super::style;

#[allow(unreachable_pub)]
#[component]
pub(super) fn StatsStrip(counted: StoredValue<Counted>) -> impl IntoView {
    let readout = RwSignal::new(None::<Readout>);

    // Drops what was counted while the strip was closed, which covers more
    // than one second.
    counted.update_value(|counted| {
        counted.read();
    });

    let reading = set_interval_with_handle(
        move || {
            let Some(read) = counted.try_update_value(Counted::read) else {
                return;
            };

            if readout.try_with_untracked(|now| now.as_ref() != Some(&read)) == Some(true) {
                readout.set(Some(read));
            }
        },
        Duration::from_secs(1),
    );

    if let Ok(reading) = reading {
        on_cleanup(move || reading.clear());
    }

    let said = move |text: fn(&Readout) -> String| {
        move || readout.with(|readout| readout.as_ref().map_or_else(|| "–".to_owned(), text))
    };

    view! {
        <div class=style::stats role="status" aria-label="Video stats">
            <span>
                <span>"FPS "</span>
                {said(|readout| readout.frames.to_string())}
            </span>
            <span>
                <span>"Latency "</span>
                {said(|readout| latency_text(readout.late))}
            </span>
            <span>
                <span>"Rate "</span>
                {said(|readout| rate_text(readout.rate))}
            </span>
        </div>
    }
}

pub(super) struct Counted {
    pub(super) frames: u32,
    pub(super) bytes: usize,
    /// The sum of how late the frames were drawn, in milliseconds.
    pub(super) late: f64,
    /// The smallest lag of a frame behind its video time on arrival. The
    /// device's clock and the page's are not synchronised, so this lag is
    /// taken as a frame that was not held up at all. None until a frame
    /// arrives on the video's clock.
    pub(super) least: Option<f64>,
}

impl Counted {
    pub(super) fn new() -> Self {
        Counted {
            frames: 0,
            bytes: 0,
            late: 0.0,
            least: None,
        }
    }

    /// Starts over when the video's clock restarts.
    pub(super) fn restart(&mut self) {
        self.least = None;
    }

    /// `behind` is in milliseconds after the frame's time on the device.
    pub(super) fn shown(&mut self, behind: f64) {
        let least = self.least.map_or(behind, |least| least.min(behind));

        self.least = Some(least);
        self.frames += 1;
        self.late += (behind - least).max(0.0);
    }

    /// `least` carries over until the video's clock restarts.
    pub(super) fn read(&mut self) -> Readout {
        let late = (self.frames > 0).then(|| (self.late / f64::from(self.frames)).round() as u32);
        let read = Readout {
            frames: self.frames,
            late,
            rate: self.bytes * 8,
        };

        self.frames = 0;
        self.bytes = 0;
        self.late = 0.0;

        read
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct Readout {
    pub(super) frames: u32,
    /// In milliseconds. None when no frame arrived.
    pub(super) late: Option<u32>,
    /// In bits per second.
    pub(super) rate: usize,
}

pub(super) fn latency_text(late: Option<u32>) -> String {
    match late {
        Some(late) => format!("+{late} ms"),
        None => "–".to_owned(),
    }
}

pub(super) fn rate_text(rate: usize) -> String {
    let rate = rate as f64;

    if rate >= 1e6 {
        format!("{:.1} Mbit/s", rate / 1e6)
    } else {
        format!("{:.0} kbit/s", rate / 1e3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_averages_and_starts_over() {
        let mut counted = Counted::new();

        counted.frames = 4;
        counted.bytes = 1000;
        counted.late = 10.0;
        counted.least = Some(3.0);

        let read = counted.read();

        assert_eq!(read.frames, 4);
        assert_eq!(read.late, Some(3));
        assert_eq!(read.rate, 8000);
        assert_eq!(counted.read().late, None);
        assert_eq!(counted.least, Some(3.0));
    }

    #[test]
    fn the_first_frame_on_a_clock_is_not_late() {
        let mut counted = Counted::new();

        counted.shown(40.0);
        counted.shown(55.0);

        assert_eq!(counted.least, Some(40.0));
        assert_eq!(counted.late, 15.0);

        counted.restart();
        counted.shown(90.0);

        assert_eq!(counted.least, Some(90.0));
        assert_eq!(counted.late, 15.0);
    }

    #[test]
    fn rates_read_in_kbit_or_mbit() {
        assert_eq!(rate_text(8000), "8 kbit/s");
        assert_eq!(rate_text(2_500_000), "2.5 Mbit/s");
        assert_eq!(latency_text(None), "–");
        assert_eq!(latency_text(Some(12)), "+12 ms");
    }
}
