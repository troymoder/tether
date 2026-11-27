use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::session::{Sample, Session};
use crate::usb::{Phone, Reads, Writes};

/// A cold start took 4 s (iOS 27.3).
pub const STARTING: Duration = Duration::from_secs(30);

/// A phone already in QuickTime mode that takes longer is taken to be stuck
/// after an earlier session, and is worth [`cycle`]. Starts took 0.2 s, and
/// the first after a cold boot 4 s (iOS 27.3).
pub const WARM: Duration = Duration::from_secs(6);

const SETTLED: Duration = Duration::from_secs(1);
const STOPPING: Duration = Duration::from_secs(5);
const LEFTOVER: Duration = Duration::from_secs(3);

/// Times are in microseconds from the capture's start, on one timeline for
/// the video and the sound.
#[derive(Debug, PartialEq)]
pub enum Media {
    /// As WebCodecs names it, such as `avc1.640033`. It comes again when the
    /// phone describes the video anew, as when the screen turns.
    Codec(String),
    /// H.264 as Annex B. A key frame starts with the parameter sets. The
    /// colours are BT.709, full range.
    Video { key: bool, time: u64, data: Vec<u8> },
    /// 16-bit stereo PCM at 48 kHz, interleaved and little-endian.
    Sound { time: u64, data: Vec<u8> },
}

/// The QuickTime configuration is made active here, so usbmux is opened on
/// it and a capture starts without the phone dropping off the bus. `fresh`
/// is for a phone this host has not had in QuickTime mode since it started.
pub async fn ready(phone: &Phone, fresh: bool) -> Result<()> {
    if fresh {
        end_leftover(phone).await?;
    }

    phone.switch_on().await?;
    phone.activate()
}

/// A second session on the same switch pings once and never streams, even
/// after the first ended cleanly (iOS 27.3). Switching off and on lets the
/// next one start.
pub async fn cycle(phone: &Phone) -> Result<()> {
    phone.switch_off().await?;
    ready(phone, false).await
}

/// Switching off puts the phone's sound back on its own speaker.
pub async fn release(phone: &Phone) -> Result<()> {
    if !phone.shows_quicktime()? {
        return Ok(());
    }

    phone.switch_off().await
}

/// Dropping it without [`Capture::stop`] leaves the phone's session open,
/// which [`ready`] ends when `fresh`.
pub struct Capture {
    session: Session<Reads, Writes>,
    started: Instant,
    video: Clock,
    sound: Clock,
    shown: u64,
    /// True after a black frame until the phone's next key frame: its frames
    /// before that refer to frames the black one ended.
    waiting: bool,
}

impl Capture {
    /// A locked phone sets the session up too, and sends frames once its
    /// screen wakes (iOS 27.3).
    pub async fn start(phone: &Phone, within: Duration) -> Result<Self> {
        let claimed = phone.claim()?;
        let mut capture = Self {
            session: Session::new(claimed.inward, claimed.outward),
            started: Instant::now(),
            video: Clock::default(),
            sound: Clock::default(),
            shown: 0,
            waiting: false,
        };
        let Ok(opened) = tokio::time::timeout(within, capture.session.opened()).await else {
            bail!("quicktime failed: the phone never answered");
        };

        opened?;

        Ok(capture)
    }

    /// A phone whose screen is off sends no frames, and none of its sound
    /// while nothing plays, for as long as it stays off.
    pub async fn next(&mut self) -> Result<Media> {
        loop {
            let sample = self.session.next().await?;
            let arrived = self.started.elapsed().as_secs_f64();

            return Ok(match sample {
                Sample::Codec(codec) => Media::Codec(codec),
                Sample::Video { key, time, data } => {
                    if self.waiting && !key {
                        continue;
                    }

                    self.waiting = false;
                    self.shown = self.video.place(arrived, time);

                    Media::Video {
                        key,
                        time: self.shown,
                        data,
                    }
                }
                Sample::Sound { time, data } => Media::Sound {
                    time: self.sound.place(arrived, time),
                    data,
                },
            });
        }
    }

    /// The phone's frames after a black one are held back until its next
    /// key frame, which comes about every half a second (iOS 27.3).
    pub fn black(&mut self) -> Option<Media> {
        let data = self.session.black()?;
        let now = (self.started.elapsed().as_secs_f64() * 1e6) as u64;

        self.waiting = true;
        self.shown = now.max(self.shown + 1);

        Some(Media::Video {
            key: true,
            time: self.shown,
            data,
        })
    }

    /// The phone stays in QuickTime mode, and the next session on it may need
    /// [`cycle`] first.
    pub async fn stop(self) {
        match tokio::time::timeout(STOPPING, self.session.stop(SETTLED)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::debug!("{error:#}"),
            Err(_) => tracing::debug!("quicktime failed: the phone did not stop"),
        }
    }
}

/// A phone still showing the QuickTime configuration was left by a host
/// that never switched it off, as a killed process does. It may still hold
/// that host's session open, and then it starts no other (iOS 27.3). The
/// session is ended and the phone switched off, so the next one starts
/// afresh.
async fn end_leftover(phone: &Phone) -> Result<()> {
    if !phone.shows_quicktime()? {
        return Ok(());
    }

    let claimed = phone.claim()?;
    let session = Session::new(claimed.inward, claimed.outward);

    match tokio::time::timeout(LEFTOVER, session.end_leftover()).await {
        Ok(Ok(())) => tracing::debug!("ended a session left over from an earlier host"),
        Ok(Err(error)) => tracing::debug!("{error:#}"),
        Err(_) => tracing::debug!("no leftover session answered"),
    }

    phone.switch_off().await
}

/// The phone's video and audio clocks start at different points, so each
/// stream's first sample is placed at its arrival, and later ones by the
/// phone's clock from there. The phone sends sound in bursts and leaves gaps
/// where nothing plays, which a count of samples would turn into drift. It
/// also restarts its audio clock near zero a few seconds into a session, so
/// a sample whose time strays from its arrival by more than [`Clock::JUMP`]
/// starts the placing afresh (iOS 27.3).
#[derive(Default)]
struct Clock {
    /// The first sample's arrival in seconds since the start, and its time
    /// on the phone's clock, since the last restart.
    from: Option<(f64, f64)>,
}

impl Clock {
    /// Well past the phone's bursts of sound, which ran a few hundred
    /// milliseconds early or late, and well short of the 3 s restart seen.
    const JUMP: f64 = 1.0;

    fn place(&mut self, arrived: f64, time: Option<f64>) -> u64 {
        let seconds = match (self.from, time) {
            (_, None) => arrived,
            (Some((from, phone_from)), Some(time))
                if (from + time - phone_from - arrived).abs() <= Self::JUMP =>
            {
                from + time - phone_from
            }
            (_, Some(time)) => {
                self.from = Some((arrived, time));
                arrived
            }
        };

        (seconds.max(0.0) * 1e6).round() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_follow_the_phones_clock_through_gaps_and_late_arrivals() {
        let mut clock = Clock::default();

        assert_eq!(clock.place(1.0, Some(100.0)), 1_000_000);
        assert_eq!(clock.place(3.8, Some(102.5)), 3_500_000);
    }

    #[test]
    fn a_restarted_phone_clock_is_placed_from_its_arrival() {
        let mut clock = Clock::default();

        clock.place(0.2, Some(0.1));
        assert_eq!(clock.place(3.3, Some(3.2)), 3_300_000);
        assert_eq!(clock.place(3.32, Some(0.28)), 3_320_000);
        assert_eq!(clock.place(3.35, Some(0.30)), 3_340_000);
    }
}
