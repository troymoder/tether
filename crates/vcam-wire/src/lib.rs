//! tetherd listens on one Unix socket (`SOCK_SEQPACKET`) for all its
//! emulators. Each packet is one rkyv-archived message, so the kernel keeps
//! message boundaries.
//!
//! On its first connection the shim sends `Join` and receives its devices in
//! `Config`. That connection stays open for the shim's log messages. Each
//! stream then has its own connection, which starts with `Hello`. Frames go
//! in memory the shim creates and hands over with `Buffers`: one slot per
//! buffer the program requested, each holding one frame. The shim gives
//! tetherd a slot to fill with `Release`, and tetherd returns it filled with
//! `Frame`. Only the side that holds a slot writes it.

use workspace_hack as _;

use std::fmt;

use rkyv::rancor::Error;
use rkyv::{Archive, Deserialize, Serialize};

pub const VERSION: u32 = 2;

/// The largest message size, in bytes.
pub const MOST: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub enum FourCc {
    Yu12,
    Nv12,
    Yuyv,
}

impl FourCc {
    pub fn code(self) -> u32 {
        u32::from_le_bytes(*self.name())
    }

    pub fn name(self) -> &'static [u8; 4] {
        match self {
            FourCc::Yu12 => b"YU12",
            FourCc::Nv12 => b"NV12",
            FourCc::Yuyv => b"YUYV",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub enum Range {
    Limited,
    Full,
}

/// The single format a device offers, fixed for as long as the program that
/// opened it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct Format {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub fourcc: FourCc,
    pub range: Range,
}

impl Format {
    /// 4:2:0 planes and 4:2:2 pairs need even sizes.
    pub fn is_valid(&self) -> bool {
        (2..=8192).contains(&self.width)
            && (2..=8192).contains(&self.height)
            && self.width.is_multiple_of(2)
            && self.height.is_multiple_of(2)
            && (1..=240).contains(&self.fps)
    }

    pub fn bytes_per_line(&self) -> u32 {
        match self.fourcc {
            FourCc::Yuyv => self.width * 2,
            FourCc::Yu12 | FourCc::Nv12 => self.width,
        }
    }

    pub fn frame_bytes(&self) -> u32 {
        match self.fourcc {
            FourCc::Yuyv => self.width * self.height * 2,
            FourCc::Yu12 | FourCc::Nv12 => self.width * self.height * 3 / 2,
        }
    }

    /// All-zero YUV is green, so luma and chroma are set.
    pub fn black(&self) -> Vec<u8> {
        let luma = match self.range {
            Range::Limited => 16,
            Range::Full => 0,
        };

        let pixels = (self.width * self.height) as usize;

        match self.fourcc {
            FourCc::Yuyv => [luma, 128].repeat(pixels),
            FourCc::Yu12 | FourCc::Nv12 => {
                let mut frame = vec![128; self.frame_bytes() as usize];

                frame[..pixels].fill(luma);

                frame
            }
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        let range = match self.range {
            Range::Limited => "limited",
            Range::Full => "full",
        };

        write!(
            out,
            "{}x{}@{}:{}:{range}",
            self.width,
            self.height,
            self.fps,
            String::from_utf8_lossy(self.fourcc.name())
        )
    }
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
pub struct Device {
    /// Such as `/dev/video0`.
    pub path: String,
    pub format: Format,
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
pub enum ToTetherd {
    Join {
        version: u32,
        emulator: String,
        pid: u32,
    },
    Hello {
        version: u32,
        emulator: String,
        pid: u32,
        device: String,
        format: Format,
    },
    /// The memory frames go in, attached as a memfd sealed against resizing:
    /// `slots` frames, each starting `slot_size` bytes after the previous one.
    /// Sent again when the program requests new buffers, and with no slots
    /// and no memfd when it frees them.
    Buffers {
        slots: u32,
        slot_size: u64,
    },
    StreamOn,
    StreamOff,
    /// tetherd may fill this slot. `generation` comes back with its frame.
    /// The shim increments it when it stops a stream or makes new buffers,
    /// and drops frames from an older generation.
    Release {
        slot: u32,
        generation: u32,
    },
    Bye,
    /// Sent on the joining connection.
    Log {
        level: Level,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
pub enum ToShim {
    Config {
        devices: Vec<Device>,
    },
    HelloOk {
        session: u32,
    },
    /// The connection closes after it.
    Err(String),
    /// The slot holds a frame and belongs to the shim again.
    Frame(Frame),
    /// The source is gone, or tetherd is stopping.
    DeviceGone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// The reason tetherd sent a frame that is not new from the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub enum Filler {
    /// The source has sent nothing for a while.
    Black,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Archive, Serialize, Deserialize)]
pub struct Frame {
    pub slot: u32,
    pub generation: u32,
    /// tetherd's frame counter, which reveals dropped frames.
    pub sequence: u32,
    /// The capture time in microseconds on the monotonic clock.
    pub micros: u64,
    /// The number of bytes of the slot it fills.
    pub size: u64,
    pub filler: Option<Filler>,
}

macro_rules! packets {
    ($($kind:ty),*) => {$(
        impl $kind {
            pub fn encode(&self) -> Vec<u8> {
                rkyv::to_bytes::<Error>(self)
                    .expect("a message is archived")
                    .to_vec()
            }

            pub fn decode(packet: &[u8]) -> Option<Self> {
                rkyv::from_bytes::<Self, Error>(packet).ok()
            }
        }
    )*};
}

packets!(ToTetherd, ToShim);

#[cfg(test)]
mod tests {
    use super::*;

    fn format(fourcc: FourCc, range: Range, width: u32, height: u32) -> Format {
        Format {
            width,
            height,
            fps: 30,
            fourcc,
            range,
        }
    }

    #[test]
    fn black_is_not_zeros() {
        let limited = format(FourCc::Yu12, Range::Limited, 4, 2).black();
        let full = format(FourCc::Yuyv, Range::Full, 2, 2).black();

        assert_eq!(
            limited,
            [16, 16, 16, 16, 16, 16, 16, 16, 128, 128, 128, 128]
        );

        assert_eq!(full, [0, 128, 0, 128, 0, 128, 0, 128]);
        assert_eq!(
            format(FourCc::Yu12, Range::Limited, 1440, 1080).to_string(),
            "1440x1080@30:YU12:limited"
        );

        assert!(!format(FourCc::Yu12, Range::Limited, 7, 5).is_valid());
        assert_eq!(FourCc::Yu12.code(), 0x3231_5559);
    }

    #[test]
    fn reads_what_it_writes() {
        let format = format(FourCc::Nv12, Range::Full, 1280, 720);

        for message in [
            ToTetherd::Join {
                version: VERSION,
                emulator: "pixel".to_owned(),
                pid: 42,
            },
            ToTetherd::Hello {
                version: VERSION,
                emulator: "pixel".to_owned(),
                pid: 42,
                device: "/dev/video1".to_owned(),
                format,
            },
            ToTetherd::Buffers {
                slots: 4,
                slot_size: 1 << 22,
            },
            ToTetherd::StreamOn,
            ToTetherd::StreamOff,
            ToTetherd::Release {
                slot: 2,
                generation: 5,
            },
            ToTetherd::Buffers {
                slots: 0,
                slot_size: 0,
            },
            ToTetherd::Bye,
            ToTetherd::Log {
                level: Level::Warn,
                message: "lost tetherd".to_owned(),
            },
        ] {
            let packet = message.encode();

            assert!(packet.len() <= MOST);
            assert_eq!(ToTetherd::decode(&packet), Some(message));
        }

        for message in [
            ToShim::Config {
                devices: vec![Device {
                    path: "/dev/video0".to_owned(),
                    format,
                }],
            },
            ToShim::HelloOk { session: 7 },
            ToShim::Err("no such lens".to_owned()),
            ToShim::Frame(Frame {
                slot: 2,
                generation: 5,
                sequence: 3,
                micros: 1 << 40,
                size: 99,
                filler: Some(Filler::Repeat),
            }),
            ToShim::DeviceGone,
        ] {
            let packet = message.encode();

            assert!(packet.len() <= MOST);
            assert_eq!(ToShim::decode(&packet), Some(message));
        }
    }
}
