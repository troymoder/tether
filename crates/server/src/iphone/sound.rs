//! The capture gives 16-bit stereo PCM at 48 kHz in pieces of its own size
//! (1024 samples on iOS 27.3), and a WebRTC track takes Opus packets of 20 ms.

use anyhow::{Result, anyhow};
use opus_pure::{Application, MAX_PACKET_BYTES, OpusEncoder};

const RATE: i32 = 48_000;
const CHANNELS: usize = 2;

/// Samples per channel in a packet: 20 ms.
const FRAME: usize = 960;

const FRAME_MICROS: u64 = 20_000;

/// Enough for music at full quality, which is what a phone mostly plays.
const BITRATE: i32 = 128_000;

pub(super) struct Sound {
    encoder: OpusEncoder,
    /// Interleaved samples not in a packet yet.
    pending: Vec<f32>,
    /// The time the first pending sample plays, in microseconds on the
    /// capture's timeline.
    from: Option<u64>,
}

impl Sound {
    pub(super) fn new() -> Result<Self> {
        let mut encoder = OpusEncoder::new(RATE, CHANNELS, Application::Audio)
            .map_err(|error| anyhow!("opus failed: {error}"))?;

        encoder.bitrate_bps = BITRATE;

        Ok(Self {
            encoder,
            pending: Vec::new(),
            from: None,
        })
    }

    /// Gives each packet the piece completed, with the time it plays.
    pub(super) fn encode(&mut self, time: u64, pcm: &[u8]) -> Vec<(u64, Vec<u8>)> {
        // A piece after a gap, or after the phone's clock started over,
        // starts the timing afresh: the samples before it are its own.
        if self.pending.is_empty() {
            self.from = Some(time);
        }

        self.pending.extend(
            pcm.as_chunks::<2>()
                .0
                .iter()
                .map(|sample| f32::from(i16::from_le_bytes(*sample)) / 32_768.0),
        );

        let mut packets = Vec::new();
        let mut packet = vec![0u8; MAX_PACKET_BYTES];

        while self.pending.len() >= FRAME * CHANNELS {
            let block: Vec<f32> = self.pending.drain(..FRAME * CHANNELS).collect();
            let from = self.from.unwrap_or(time);

            self.from = Some(from + FRAME_MICROS);

            match self.encoder.encode(&block, FRAME, &mut packet) {
                Ok(written) => packets.push((from, packet[..written].to_vec())),
                Err(error) => tracing::debug!("opus failed: {error}"),
            }
        }

        packets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pieces_become_20_ms_packets_on_the_capture_timeline() {
        let mut sound = Sound::new().unwrap();
        // 1024 stereo samples of silence, as the phone sends them.
        let piece = vec![0u8; 1024 * 4];

        let first = sound.encode(1_000_000, &piece);
        let second = sound.encode(1_021_333, &piece);

        assert_eq!(first.len(), 1);
        assert_eq!(first[0].0, 1_000_000);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].0, 1_020_000);
        assert!(!first[0].1.is_empty());
    }
}
