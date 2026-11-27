//! Rewrites an SPS for a browser's decoder, in two ways an iPhone's screen
//! stream needs (iOS 27.3):
//!
//! - It promises that frames are never reordered, as WebRTC's
//!   `SpsVuiRewriter` does. Without `bitstream_restriction` in its VUI, a
//!   High profile SPS leaves a decoder free to hold frames back until its
//!   buffer fills, and a screen that sends a frame only when something
//!   changes then shows each one late. The phone sends no B-frames, so the
//!   promise is true.
//! - It names BT.709 colours where the SPS gives full range but leaves the
//!   primaries, transfer and matrix unspecified, which a browser otherwise
//!   guesses at and shows washed out.
//!
//! The SPS is read field by field (ITU-T H.264 7.3.2.1.1, E.1.1), because
//! scuffle-h264's `Sps` reads and writes the VUI only as far as its timing
//! info, so everything after that would be lost. Everything before
//! `bitstream_restriction_flag` is kept bit for bit apart from the colour
//! codes, which keep their length.

use std::io::{self, Cursor, Read, Write};

use scuffle_bytes_util::{BitReader, BitWriter, EmulationPreventionIo};
use scuffle_expgolomb::{BitReaderExpGolombExt, BitWriterExpGolombExt};

type Reader<'a> = BitReader<Cursor<&'a [u8]>>;

fn skip_bits(reader: &mut Reader<'_>, count: u8) -> io::Result<()> {
    reader.read_bits(count).map(|_| ())
}

fn skip_ue(reader: &mut Reader<'_>) -> io::Result<()> {
    reader.read_exp_golomb().map(|_| ())
}

fn scaling_list(reader: &mut Reader<'_>, size: usize) -> io::Result<()> {
    let mut last = 8i64;
    let mut next = 8i64;

    for _ in 0..size {
        if next != 0 {
            next = (last + reader.read_signed_exp_golomb()? + 256) % 256;
        }

        if next != 0 {
            last = next;
        }
    }

    Ok(())
}

fn hrd(reader: &mut Reader<'_>) -> io::Result<()> {
    let count = reader.read_exp_golomb()? + 1;

    skip_bits(reader, 8)?;

    for _ in 0..count {
        skip_ue(reader)?;
        skip_ue(reader)?;
        reader.read_bit()?;
    }

    skip_bits(reader, 20)
}

/// The three colour codes, each "unspecified" (2).
const UNSPECIFIED: u64 = 0x02_02_02;

/// BT.709 for each (1), which Apple's encoders use for video from HD up.
const BT709: u64 = 0x01_01_01;

/// Positions are in bits from the RBSP's start.
struct Found {
    /// The position `bitstream_restriction_flag` would take, and whether the SPS has
    /// a VUI. None if it is restricted already.
    restriction: Option<(u64, bool)>,
    /// The position of `colour_primaries`, when it and the transfer and matrix
    /// after it are all unspecified.
    unspecified: Option<u64>,
    references: u64,
}

fn walk(reader: &mut Reader<'_>) -> io::Result<Found> {
    let profile = reader.read_bits(8)?;

    skip_bits(reader, 16)?;
    skip_ue(reader)?;

    if matches!(
        profile,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    ) {
        let chroma = reader.read_exp_golomb()?;

        if chroma == 3 {
            reader.read_bit()?;
        }

        skip_ue(reader)?;
        skip_ue(reader)?;
        reader.read_bit()?;

        if reader.read_bit()? {
            for index in 0..if chroma == 3 { 12 } else { 8 } {
                if reader.read_bit()? {
                    scaling_list(reader, if index < 6 { 16 } else { 64 })?;
                }
            }
        }
    }

    skip_ue(reader)?;

    match reader.read_exp_golomb()? {
        0 => skip_ue(reader)?,
        1 => {
            reader.read_bit()?;
            reader.read_signed_exp_golomb()?;
            reader.read_signed_exp_golomb()?;

            for _ in 0..reader.read_exp_golomb()? {
                reader.read_signed_exp_golomb()?;
            }
        }
        _ => {}
    }

    let references = reader.read_exp_golomb()?;

    reader.read_bit()?;
    skip_ue(reader)?;
    skip_ue(reader)?;

    if !reader.read_bit()? {
        reader.read_bit()?;
    }

    reader.read_bit()?;

    if reader.read_bit()? {
        for _ in 0..4 {
            skip_ue(reader)?;
        }
    }

    let vui_flag = reader.bit_stream_position()?;

    if !reader.read_bit()? {
        return Ok(Found {
            restriction: Some((vui_flag, false)),
            unspecified: None,
            references,
        });
    }

    if reader.read_bit()? && reader.read_bits(8)? == 255 {
        skip_bits(reader, 32)?;
    }

    if reader.read_bit()? {
        reader.read_bit()?;
    }

    let mut unspecified = None;

    if reader.read_bit()? {
        // video_format and video_full_range_flag.
        skip_bits(reader, 4)?;

        if reader.read_bit()? {
            let at = reader.bit_stream_position()?;

            if reader.read_bits(24)? == UNSPECIFIED {
                unspecified = Some(at);
            }
        }
    }

    if reader.read_bit()? {
        skip_ue(reader)?;
        skip_ue(reader)?;
    }

    if reader.read_bit()? {
        skip_bits(reader, 32)?;
        skip_bits(reader, 32)?;
        reader.read_bit()?;
    }

    let nal = reader.read_bit()?;

    if nal {
        hrd(reader)?;
    }

    let vcl = reader.read_bit()?;

    if vcl {
        hrd(reader)?;
    }

    if nal || vcl {
        reader.read_bit()?;
    }

    reader.read_bit()?;

    let at = reader.bit_stream_position()?;
    let restriction = (!reader.read_bit()?).then_some((at, true));

    Ok(Found {
        restriction,
        unspecified,
        references,
    })
}

/// `unit` has its NAL header. None if it cannot be read or needs neither
/// change, when it is left as it is.
pub(crate) fn prepared(unit: &[u8]) -> Option<Vec<u8>> {
    let mut rbsp = Vec::new();

    EmulationPreventionIo::new(unit.get(1..)?)
        .read_to_end(&mut rbsp)
        .ok()?;

    let found = walk(&mut BitReader::new(Cursor::new(&rbsp[..]))).ok()?;

    if found.restriction.is_none() && found.unspecified.is_none() {
        return None;
    }

    if let Some(at) = found.unspecified {
        rbsp = patched(&rbsp, at, BT709).ok()?;
    }

    let written = match found.restriction {
        Some((at, has_vui)) => restricted(&rbsp, at, has_vui, found.references).ok()?,
        None => rbsp,
    };
    let mut escaped = vec![unit[0]];

    EmulationPreventionIo::new(&mut escaped)
        .write_all(&written)
        .ok()?;

    Some(escaped)
}

fn patched(rbsp: &[u8], at: u64, value: u64) -> io::Result<Vec<u8>> {
    let mut copy = BitReader::new(Cursor::new(rbsp));
    let mut writer = BitWriter::<Vec<u8>>::default();
    let total = rbsp.len() as u64 * 8;

    for _ in 0..at {
        writer.write_bit(copy.read_bit()?)?;
    }

    copy.read_bits(24)?;
    writer.write_bits(value, 24)?;

    for _ in at + 24..total {
        writer.write_bit(copy.read_bit()?)?;
    }

    writer.finish()
}

fn restricted(rbsp: &[u8], at: u64, has_vui: bool, references: u64) -> io::Result<Vec<u8>> {
    let mut copy = BitReader::new(Cursor::new(rbsp));
    let mut writer = BitWriter::<Vec<u8>>::default();

    for _ in 0..at {
        writer.write_bit(copy.read_bit()?)?;
    }

    if !has_vui {
        // A VUI with nothing in it but the restriction: its eight presence
        // flags before it, from aspect ratio to picture structure, clear.
        writer.write_bit(true)?;
        writer.write_bits(0, 8)?;
    }

    // bitstream_restriction_flag, motion vectors over picture boundaries,
    // max bytes per picture and bits per macroblock, the longest motion
    // vectors, no reordering, and a buffer of the reference frames.
    writer.write_bit(true)?;
    writer.write_bit(true)?;
    writer.write_exp_golomb(2)?;
    writer.write_exp_golomb(1)?;
    writer.write_exp_golomb(16)?;
    writer.write_exp_golomb(16)?;
    writer.write_exp_golomb(0)?;
    writer.write_exp_golomb(references)?;
    writer.write_bit(true)?;
    writer.align()?;
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SPS an iPhone on iOS 27.3 sends with its screen, 1296×2816 High
    /// 5.1, with a VUI and no restriction.
    const IPHONE: [u8; 17] = [
        0x27, 0x64, 0x00, 0x33, 0x4b, 0x04, 0xc5, 0x14, 0x05, 0x10, 0x16, 0x1a, 0x6e, 0x04, 0x04,
        0x04, 0x04,
    ];

    #[test]
    fn the_iphones_sps_is_prepared_once() {
        let rewritten = prepared(&IPHONE).unwrap();

        // Checked with ffmpeg's trace_headers: full range with BT.709
        // primaries, transfer and matrix, bitstream_restriction_flag 1,
        // max_num_reorder_frames 0 and max_dec_frame_buffering 4. The
        // colour codes sit a bit off the bytes, so 1, 1, 1 shows as 02.
        assert_eq!(
            rewritten,
            [
                0x27, 0x64, 0x00, 0x33, 0x4b, 0x04, 0xc5, 0x14, 0x05, 0x10, 0x16, 0x1a, 0x6e, 0x02,
                0x02, 0x02, 0x0d, 0xa0, 0x88, 0x46, 0x58
            ]
        );
        assert_eq!(prepared(&rewritten), None);
    }
}
