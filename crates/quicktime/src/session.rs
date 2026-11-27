//! The phone pings, both sides set up clocks for the video and sound, and
//! the phone sends a CMSampleBuffer of H.264 for each NEED this host sends,
//! and its sound as it plays.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};

use crate::video::Avcc;
use crate::wire;

pub(crate) trait Inward {
    fn read(&mut self) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

pub(crate) trait Outward {
    fn write(&mut self, data: Vec<u8>) -> impl Future<Output = Result<()>> + Send;
}

/// The names of this host's clocks, as offsets from the phone's: qvh's
/// numbers, which the phone takes.
const AUDIO_CLOCK: u64 = 1000;
const VIDEO_CLOCK: u64 = 0x1000af;
const TIME_CLOCK: u64 = 0x10000;

/// The answer to every SKEW, the sound's nominal sample rate. A rate
/// measured from the sound's arrival, as qvh gives, wavered by 0.1% and the
/// phone's player then held pictures and skipped ahead about once a second
/// (iOS 27.3, research/runs/stall-* in ios-qvh). The flat rate stopped most
/// of it.
const RATE: f64 = 48_000.0;

#[derive(Debug, PartialEq)]
pub(crate) enum Sample {
    /// As WebCodecs names it.
    Codec(String),
    /// `time` is in seconds on the phone's video clock.
    Video {
        key: bool,
        time: Option<f64>,
        data: Vec<u8>,
    },
    /// `time` is in seconds on the phone's audio clock.
    Sound { time: Option<f64>, data: Vec<u8> },
}

pub(crate) struct Session<I, O> {
    inward: I,
    outward: O,
    received: Vec<u8>,
    waiting: VecDeque<Sample>,
    need: Option<Vec<u8>>,
    audio_clock: u64,
    /// The clock the phone asked for in CLOK, whose time TIME asks.
    clock: Instant,
    video: Avcc,
    codec: Option<String>,
    released: u32,
    running: bool,
}

impl<I: Inward, O: Outward> Session<I, O> {
    pub(crate) fn new(inward: I, outward: O) -> Self {
        Self {
            inward,
            outward,
            received: Vec::new(),
            waiting: VecDeque::new(),
            need: None,
            audio_clock: 0,
            clock: Instant::now(),
            video: Avcc::default(),
            codec: None,
            released: 0,
            running: true,
        }
    }

    pub(crate) fn black(&mut self) -> Option<Vec<u8>> {
        self.video.black()
    }

    /// Waits for the phone's sound clock (CWPA) or its video (CVRP). A
    /// locked phone sets up the sound at once and the video only when its
    /// screen wakes (iOS 27.3).
    pub(crate) async fn opened(&mut self) -> Result<()> {
        while self.need.is_none() && self.audio_clock == 0 {
            let message = self.message().await?;

            self.handle(&message).await?;
        }

        Ok(())
    }

    pub(crate) async fn next(&mut self) -> Result<Sample> {
        loop {
            if let Some(sample) = self.waiting.pop_front() {
                return Ok(sample);
            }

            let message = self.message().await?;

            self.handle(&message).await?;
        }
    }

    /// HPA0 and HPD0, then HPD0 again once the phone released both clocks,
    /// as qvh does. It goes on reading and answering until the phone has
    /// been quiet for `quiet`, so that nothing it sends is left waiting on
    /// its side when the interface is let go. Bound it with a timeout,
    /// because a phone that is gone never answers.
    pub(crate) async fn stop(mut self, quiet: Duration) -> Result<()> {
        self.running = false;
        self.released = 0;

        let audio = wire::hpa0(self.audio_clock);

        self.send(audio).await?;
        self.send(wire::hpd0()).await?;

        while self.released < 2 {
            let message = self.message().await?;

            self.handle(&message).await?;
        }

        self.send(wire::hpd0()).await?;

        while let Ok(message) = tokio::time::timeout(quiet, self.message()).await {
            self.handle(&message?).await?;
        }

        Ok(())
    }

    /// HPA0 and HPD0 end a session an earlier host left open whatever its
    /// clocks, and the phone answers by releasing them. Bound it with a
    /// timeout.
    pub(crate) async fn end_leftover(mut self) -> Result<()> {
        self.running = false;
        self.send(wire::hpa0(0)).await?;
        self.send(wire::hpd0()).await?;

        while self.released < 2 {
            let message = self.message().await?;

            self.handle(&message).await?;
        }

        Ok(())
    }

    async fn send(&mut self, message: Vec<u8>) -> Result<()> {
        self.outward.write(message).await
    }

    async fn message(&mut self) -> Result<Vec<u8>> {
        loop {
            if let Some(length) = self.received.get(..4) {
                let length = u32::from_le_bytes(length.try_into()?) as usize;

                if length < 4 {
                    bail!("quicktime failed: a message of {length} bytes");
                }

                if self.received.len() >= length {
                    let message = self.received[4..length].to_vec();

                    self.received.drain(..length);

                    return Ok(message);
                }
            }

            let data = self.inward.read().await?;

            self.received.extend(data);
        }
    }

    async fn handle(&mut self, data: &[u8]) -> Result<()> {
        let message =
            wire::Message::parse(data).ok_or_else(|| anyhow!("quicktime failed: a cut message"))?;

        // Frames and sound come many times a second, and everything else
        // rarely, so the rest can be read at debug.
        if message.kind == wire::ASYN && matches!(message.what, wire::FEED | wire::EAT) {
            tracing::trace!(
                "received {} {}, {} bytes",
                wire::letters(message.kind),
                wire::letters(message.what),
                data.len()
            );
        } else {
            tracing::debug!(
                "received {} {} on {:x}, {} bytes: {}",
                wire::letters(message.kind),
                wire::letters(message.what),
                message.clock,
                data.len(),
                hex(data.get(16..data.len().min(80)).unwrap_or_default())
            );
        }

        match message.kind {
            wire::PING => self.send(wire::ping()).await,
            wire::SYNC => self.sync(&message).await,
            wire::ASYN => self.asyn(&message).await,
            other => bail!("quicktime failed: unknown message {}", wire::letters(other)),
        }
    }

    async fn sync(&mut self, message: &wire::Message<'_>) -> Result<()> {
        let correlation = message
            .correlation()
            .ok_or_else(|| anyhow!("quicktime failed: a cut sync"))?;

        match message.what {
            // This host says it is a display and a speaker, which takes the
            // sound off the phone's own speaker, as scrcpy's capture does on
            // Android.
            wire::CWPA => {
                let audio = message
                    .named_clock()
                    .ok_or_else(|| anyhow!("quicktime failed: a cut cwpa"))?;

                self.audio_clock = audio;

                // qvh sends this twice, and the phone has only been seen to
                // start with that.
                self.send(wire::hpd1()).await?;
                self.send(wire::hpd1()).await?;
                self.send(wire::reply_clock(correlation, audio + AUDIO_CLOCK))
                    .await?;
                self.send(wire::hpa1(audio)).await
            }
            wire::AFMT => self.send(wire::reply_format(correlation)).await,
            wire::CVRP => {
                let video = message
                    .named_clock()
                    .ok_or_else(|| anyhow!("quicktime failed: a cut cvrp"))?;
                let need = wire::need(video);

                if let Some(avcc) = wire::cvrp_avcc(message) {
                    self.describe(avcc);
                }

                self.send(need.clone()).await?;
                self.need = Some(need);
                self.send(wire::reply_clock(correlation, video + VIDEO_CLOCK))
                    .await
            }
            wire::CLOK => {
                self.clock = Instant::now();
                self.send(wire::reply_clock(correlation, message.clock + TIME_CLOCK))
                    .await
            }
            wire::TIME => {
                let elapsed = self.clock.elapsed().as_nanos() as u64;

                self.send(wire::reply_time(correlation, elapsed)).await
            }
            wire::SKEW => self.send(wire::reply_skew(correlation, RATE)).await,
            _ => self.send(wire::reply_empty(correlation)).await,
        }
    }

    async fn asyn(&mut self, message: &wire::Message<'_>) -> Result<()> {
        match message.what {
            wire::FEED => {
                if let Some(need) = self.need.clone()
                    && self.running
                {
                    self.send(need).await?;
                }

                let Some(sample) = wire::sample(message) else {
                    return Ok(());
                };

                if let Some(avcc) = sample.avcc {
                    self.describe(avcc);
                }

                if let Some((key, data)) = self.video.frame(sample.data) {
                    self.waiting.push_back(Sample::Video {
                        key,
                        time: sample.time,
                        data,
                    });
                }
            }
            wire::EAT => {
                if let Some(sample) = wire::sample(message) {
                    self.waiting.push_back(Sample::Sound {
                        time: sample.time,
                        data: sample.data.to_vec(),
                    });
                }
            }
            wire::RELS => self.released += 1,
            _ => {}
        }

        Ok(())
    }

    /// The codec is handed on whenever a description comes, since the
    /// frames after it may differ in size.
    fn describe(&mut self, avcc: &[u8]) {
        if self.video.describe(avcc)
            && let Some(codec) = self.video.codec()
        {
            self.codec = Some(codec.to_owned());
            self.waiting.push_back(Sample::Codec(codec.to_owned()));
        }
    }
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    struct Phone(VecDeque<Vec<u8>>);

    impl Inward for Phone {
        async fn read(&mut self) -> Result<Vec<u8>> {
            self.0.pop_front().ok_or_else(|| anyhow!("gone"))
        }
    }

    #[derive(Clone, Default)]
    struct Sent(Arc<Mutex<Vec<Vec<u8>>>>);

    impl Outward for Sent {
        async fn write(&mut self, data: Vec<u8>) -> Result<()> {
            self.0.lock().unwrap().push(data);

            Ok(())
        }
    }

    #[tokio::test]
    async fn a_session_opens_and_gives_the_codec_then_a_key_frame() {
        let stream = [
            wire::ping(),
            fixture("cwpa-request1"),
            fixture("cvrp-request"),
            fixture("asyn-feed"),
        ]
        .concat();
        let pieces = stream.chunks(1000).map(<[u8]>::to_vec).collect();
        let sent = Sent::default();
        let mut session = Session::new(Phone(pieces), sent.clone());

        assert!(
            matches!(session.next().await.unwrap(), Sample::Codec(codec) if codec.starts_with("avc1.64"))
        );

        // The frame's own format description is handed on again.
        let mut next = session.next().await.unwrap();

        if matches!(next, Sample::Codec(_)) {
            next = session.next().await.unwrap();
        }

        let Sample::Video { key, time, data } = next else {
            panic!("no frame");
        };

        assert!(key);
        assert!(time.is_some());
        assert_eq!(&data[..5], [0, 0, 0, 1, 0x27]);
        assert!(data.len() > 90_712);

        let sent = sent.0.lock().unwrap();
        let what: Vec<u32> = sent
            .iter()
            .map(|message| u32::from_le_bytes(message[4..8].try_into().unwrap()))
            .collect();

        // PING, then two HPD1, the reply and HPA1 for CWPA, then NEED and the
        // reply for CVRP, then NEED for the frame.
        assert_eq!(
            what,
            [
                wire::PING,
                wire::ASYN,
                wire::ASYN,
                wire::RPLY,
                wire::ASYN,
                wire::ASYN,
                wire::RPLY,
                wire::ASYN
            ]
        );
        assert_eq!(sent[1], fixture("asyn-hpd1"));
    }
}
