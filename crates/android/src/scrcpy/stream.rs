use std::fmt::Debug;

use adb::Channel;
use adb::libadb::Session;
use anyhow::{Result, bail, ensure};

/// The flags in the first field of a packet's header. The remaining bits
/// are the timestamp.
const SESSION: u64 = 1 << 63;
const CONFIG: u64 = 1 << 62;
const KEY: u64 = 1 << 61;
const TIME: u64 = KEY - 1;

#[derive(Clone, Debug, PartialEq)]
pub enum Picture {
    /// As WebCodecs names it (`avc1.640028`). It arrives before the first
    /// frame and again when the video changes size, such as when the screen
    /// rotates.
    Codec(String),
    /// One H.264 frame in Annex B form. A key frame carries its own
    /// parameter sets. `time` is its presentation time in microseconds.
    Frame { key: bool, time: u64, data: Vec<u8> },
}

pub struct Pictures<S> {
    stream: Stream<S>,
    framer: Framer,
}

impl<S> Pictures<S>
where
    S: Session,
    S::IoError: Debug,
{
    pub(super) fn new(stream: Stream<S>) -> Self {
        Pictures {
            stream,
            framer: Framer::default(),
        }
    }

    pub async fn next(&mut self) -> Result<Option<Picture>> {
        loop {
            let Some((flags, data)) = self.stream.packet(1 << 24).await? else {
                return Ok(None);
            };

            if let Some(picture) = self.framer.packet(flags, data) {
                return Ok(Some(picture));
            }
        }
    }
}

/// Opus at 48 kHz in stereo.
pub struct Sound<S> {
    stream: Stream<S>,
    started: bool,
}

impl<S> Sound<S>
where
    S: Session,
    S::IoError: Debug,
{
    pub(super) fn new(stream: Stream<S>) -> Self {
        Sound {
            stream,
            started: false,
        }
    }

    /// Returns each packet with its timestamp in microseconds. Fails at once
    /// on a device with no audio, and the screen carries on without it.
    pub async fn next(&mut self) -> Result<Option<(u64, Vec<u8>)>> {
        if !self.started {
            let Some(codec) = self.stream.exact(4).await? else {
                return Ok(None);
            };

            // scrcpy sends codec 0 when audio is disabled on the device, and
            // 1 when capturing it failed.
            ensure!(
                u32::from_be_bytes(codec.try_into().unwrap_or_default()) > 1,
                "scrcpy failed: no audio capture"
            );

            self.started = true;
        }

        loop {
            let Some((flags, data)) = self.stream.packet(1 << 20).await? else {
                return Ok(None);
            };

            // The config packet is the same for every Opus stream, and the
            // browser does not need it.
            if flags & CONFIG == 0 {
                return Ok(Some((flags & TIME, data)));
            }
        }
    }
}

pub struct Replies<S> {
    stream: Stream<S>,
}

impl<S> Replies<S>
where
    S: Session,
    S::IoError: Debug,
{
    pub(super) fn new(stream: Stream<S>) -> Self {
        Replies { stream }
    }

    pub async fn next(&mut self) -> Result<Option<String>> {
        loop {
            let Some(kind) = self.stream.exact(1).await? else {
                return Ok(None);
            };

            let skipped = match kind[0] {
                0 => {
                    let Some(length) = self.stream.exact(4).await? else {
                        return Ok(None);
                    };

                    let length = u32::from_be_bytes(length.try_into().unwrap_or_default());

                    let Some(text) = self.stream.exact(length as usize).await? else {
                        return Ok(None);
                    };

                    return Ok(Some(String::from_utf8_lossy(&text).into_owned()));
                }
                // An acknowledgement that the clipboard was set, which
                // `control_message` never requests.
                1 => self.stream.exact(8).await?,
                // Output for a virtual keyboard or mouse: a 4-byte header
                // ending in the length, then the data.
                2 => match self.stream.exact(4).await? {
                    Some(head) => {
                        self.stream
                            .exact(usize::from(u16::from_be_bytes([head[2], head[3]])))
                            .await?
                    }
                    None => None,
                },
                other => bail!("scrcpy failed: unknown message type {other}"),
            };

            if skipped.is_none() {
                return Ok(None);
            }
        }
    }
}

pub(super) struct Stream<S> {
    pub(super) channel: Channel<S>,
    came: Vec<u8>,
}

impl<S> Stream<S>
where
    S: Session,
    S::IoError: Debug,
{
    pub(super) fn new(channel: Channel<S>) -> Self {
        Stream {
            channel,
            came: Vec::new(),
        }
    }

    pub(super) async fn exact(&mut self, length: usize) -> Result<Option<Vec<u8>>> {
        while self.came.len() < length {
            let mut more = vec![0; 64 * 1024];
            let got = self.channel.read(&mut more).await?;

            if got == 0 {
                return Ok(None);
            }

            self.came.extend_from_slice(&more[..got]);
        }

        Ok(Some(self.came.drain(..length).collect()))
    }

    /// Returns a packet's first header field, which holds the flags and the
    /// timestamp, and its data of at most `most` bytes.
    async fn packet(&mut self, most: usize) -> Result<Option<(u64, Vec<u8>)>> {
        let Some(header) = self.exact(12).await? else {
            return Ok(None);
        };

        let (flags, length) = header_of(&header)?;

        ensure!(length <= most, "scrcpy failed: a {length} byte packet");

        let Some(data) = self.exact(length).await? else {
            return Ok(None);
        };

        Ok(Some((flags, data)))
    }
}

/// A session packet, sent at the start and on every size change, has no data
/// because its last eight bytes hold the video size.
fn header_of(header: &[u8]) -> Result<(u64, usize)> {
    let flags = u64::from_be_bytes(header[..8].try_into()?);

    if flags & SESSION != 0 {
        return Ok((flags, 0));
    }

    Ok((
        flags,
        u32::from_be_bytes(header[8..12].try_into()?) as usize,
    ))
}

#[derive(Default)]
struct Framer {
    /// The video's parameter sets, which scrcpy sends in a separate packet
    /// before the first frame and again when the size changes.
    config: Vec<u8>,
}

impl Framer {
    fn packet(&mut self, flags: u64, mut data: Vec<u8>) -> Option<Picture> {
        // The config packet that follows carries the new size too.
        if flags & SESSION != 0 {
            return None;
        }

        if flags & CONFIG != 0 {
            let codec = codec(&data)?;

            self.config = data;

            return Some(Picture::Codec(codec));
        }

        let key = flags & KEY != 0;

        // The browser's decoder needs the parameter sets inside the key frame.
        if key {
            data.splice(..0, self.config.iter().copied());
        }

        Some(Picture::Frame {
            key,
            time: flags & TIME,
            data,
        })
    }
}

/// `avc1.` and the sequence parameter set's profile, constraints and level in
/// hex.
fn codec(config: &[u8]) -> Option<String> {
    // A NAL unit starts after 00 00 01, and type 7 is the sequence parameter set.
    let start = config
        .windows(4)
        .position(|bytes| bytes[..3] == [0, 0, 1] && bytes[3] & 0x1f == 7)?;
    let sps = config.get(start + 4..start + 7)?;

    Some(format!("avc1.{:02x}{:02x}{:02x}", sps[0], sps[1], sps[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The start of a real scrcpy config packet: a sequence parameter set for
    // the High profile at level 4.0.
    const CONFIG_PACKET: [u8; 12] = [0, 0, 0, 1, 0x67, 0x64, 0x00, 0x28, 0xac, 0, 0, 1];

    #[test]
    fn names_the_codec() {
        assert_eq!(codec(&CONFIG_PACKET).as_deref(), Some("avc1.640028"));
        assert_eq!(codec(&[0, 0, 1, 0x65, 1, 2, 3]), None);
        assert_eq!(codec(&[0, 0, 1, 0x67, 1]), None);
    }

    #[test]
    fn reads_the_headers() {
        // A session packet, resized by the client: 1280 by 720, of which
        // the width is in the first field, and no data.
        let session = [0x80, 0, 0, 1, 0, 0, 5, 0, 0, 0, 2, 0xd0];

        assert_eq!(header_of(&session).unwrap(), (SESSION | 1 << 32 | 1280, 0));

        let frame = [0x20, 0, 0, 0, 0, 0, 0, 7, 0, 0, 1, 0x2c];

        assert_eq!(header_of(&frame).unwrap(), (KEY | 7, 300));
    }

    #[test]
    fn frames_the_stream() {
        let mut framer = Framer::default();

        assert_eq!(framer.packet(SESSION, Vec::new()), None);
        assert_eq!(
            framer.packet(CONFIG, CONFIG_PACKET.to_vec()),
            Some(Picture::Codec("avc1.640028".to_owned()))
        );

        let mut data = CONFIG_PACKET.to_vec();

        data.extend([9, 9]);

        let key = Picture::Frame {
            key: true,
            time: 5,
            data,
        };

        assert_eq!(framer.packet(KEY | 5, vec![9, 9]), Some(key));

        let delta = Picture::Frame {
            key: false,
            time: 6,
            data: vec![8],
        };

        assert_eq!(framer.packet(6, vec![8]), Some(delta));
    }
}
