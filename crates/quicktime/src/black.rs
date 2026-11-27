//! A black key frame for the phone's stream, shown while its screen is off,
//! when the phone sends no frames (ITU-T H.264 7.3).
//!
//! It goes with a PPS of its own, since the phone codes its frames with
//! CABAC, which would take an arithmetic coder to write. This one uses CAVLC
//! and refers to the phone's SPS, so the frame has the stream's size and the
//! slice header follows the SPS's fields. The first macroblock is I_PCM with
//! black samples, and every other one predicts its DC from black neighbours
//! (Intra 16x16, DC, no residual), which leaves about a byte per macroblock.

use std::io::{self, Write};

use scuffle_bytes_util::{BitWriter, EmulationPreventionIo};
use scuffle_expgolomb::BitWriterExpGolombExt;

const START: [u8; 4] = [0, 0, 0, 1];

/// The PPS's id, out of the way of the phone's own, which is 0.
const PPS_ID: u64 = 255;

const PPS_HEADER: u8 = 0x68;
const IDR_HEADER: u8 = 0x65;

/// mb_type in an I slice: I_PCM, and Intra 16x16 with DC prediction and no
/// coded blocks (Table 7-11).
const I_PCM: u64 = 25;
const I_16X16_DC: u64 = 3;

const BLACK: u64 = 0;
const NO_COLOUR: u64 = 128;

struct Shape {
    sps_id: u64,
    frame_num_bits: u8,
    order: Order,
    width: usize,
    height: usize,
}

enum Order {
    /// pic_order_cnt_type 0, with the LSB's bit count.
    Lsb(u8),
    /// pic_order_cnt_type 1 without the always zero flag, which puts a delta
    /// in the slice header.
    Delta,
    None,
}

/// `idr` tells consecutive IDRs apart. None for a stream that is not 8-bit
/// 4:2:0 or not progressive.
pub(crate) fn frame(sps: &[u8], idr: u16) -> Option<Vec<u8>> {
    let shape = shape(sps)?;
    let mut out = Vec::new();

    for (header, rbsp) in [
        (PPS_HEADER, pps(&shape).ok()?),
        (IDR_HEADER, slice(&shape, idr).ok()?),
    ] {
        out.extend(START);
        out.push(header);
        EmulationPreventionIo::new(&mut out).write_all(&rbsp).ok()?;
    }

    Some(out)
}

fn shape(sps: &[u8]) -> Option<Shape> {
    let sps = scuffle_h264::Sps::parse_with_emulation_prevention(sps).ok()?;

    if let Some(ext) = &sps.ext
        && (ext.chroma_format_idc != 1
            || ext.separate_color_plane_flag
            || ext.bit_depth_luma_minus8 != 0
            || ext.bit_depth_chroma_minus8 != 0)
    {
        return None;
    }

    if sps.mb_adaptive_frame_field_flag.is_some() {
        return None;
    }

    let order = match sps.pic_order_cnt_type {
        0 => Order::Lsb(sps.log2_max_pic_order_cnt_lsb_minus4? + 4),
        1 if sps
            .pic_order_cnt_type1
            .as_ref()
            .is_some_and(|one| !one.delta_pic_order_always_zero_flag) =>
        {
            Order::Delta
        }
        _ => Order::None,
    };

    Some(Shape {
        sps_id: u64::from(sps.seq_parameter_set_id),
        frame_num_bits: sps.log2_max_frame_num_minus4 + 4,
        order,
        width: usize::try_from(sps.pic_width_in_mbs_minus1 + 1).ok()?,
        height: usize::try_from(sps.pic_height_in_map_units_minus1 + 1).ok()?,
    })
}

fn pps(shape: &Shape) -> io::Result<Vec<u8>> {
    let mut writer = BitWriter::<Vec<u8>>::default();

    writer.write_exp_golomb(PPS_ID)?;
    writer.write_exp_golomb(shape.sps_id)?;
    // entropy_coding_mode_flag: CAVLC, and
    // bottom_field_pic_order_in_frame_present_flag.
    writer.write_bit(false)?;
    writer.write_bit(false)?;
    // num_slice_groups_minus1, and num_ref_idx_l0 and _l1_default_active_minus1.
    writer.write_exp_golomb(0)?;
    writer.write_exp_golomb(0)?;
    writer.write_exp_golomb(0)?;
    // weighted_pred_flag, weighted_bipred_idc.
    writer.write_bit(false)?;
    writer.write_bits(0, 2)?;
    // pic_init_qp_minus26, pic_init_qs_minus26, chroma_qp_index_offset.
    writer.write_signed_exp_golomb(0)?;
    writer.write_signed_exp_golomb(0)?;
    writer.write_signed_exp_golomb(0)?;
    // deblocking_filter_control_present_flag, constrained_intra_pred_flag,
    // redundant_pic_cnt_present_flag.
    writer.write_bit(false)?;
    writer.write_bit(false)?;
    writer.write_bit(false)?;
    trailing(&mut writer)?;
    writer.finish()
}

fn slice(shape: &Shape, idr: u16) -> io::Result<Vec<u8>> {
    let mut writer = BitWriter::<Vec<u8>>::default();

    // first_mb_in_slice, slice_type I (all slices I), pic_parameter_set_id.
    writer.write_exp_golomb(0)?;
    writer.write_exp_golomb(7)?;
    writer.write_exp_golomb(PPS_ID)?;
    // frame_num, 0 in an IDR.
    writer.write_bits(0, shape.frame_num_bits)?;
    writer.write_exp_golomb(u64::from(idr))?;

    match shape.order {
        Order::Lsb(bits) => writer.write_bits(0, bits)?,
        Order::Delta => writer.write_signed_exp_golomb(0)?,
        Order::None => {}
    }

    // dec_ref_pic_marking: no_output_of_prior_pics_flag,
    // long_term_reference_flag.
    writer.write_bit(false)?;
    writer.write_bit(false)?;
    // slice_qp_delta.
    writer.write_signed_exp_golomb(0)?;

    for at in 0..shape.width * shape.height {
        if at == 0 {
            pcm(&mut writer)?;
        } else {
            dc(&mut writer, shape, at)?;
        }
    }

    trailing(&mut writer)?;
    writer.finish()
}

fn pcm(writer: &mut BitWriter<Vec<u8>>) -> io::Result<()> {
    writer.write_exp_golomb(I_PCM)?;
    // pcm_alignment_zero_bit.
    writer.align()?;

    for _ in 0..256 {
        writer.write_bits(BLACK, 8)?;
    }

    for _ in 0..128 {
        writer.write_bits(NO_COLOUR, 8)?;
    }

    Ok(())
}

/// With no residual, only the luma DC block's coeff_token is written, for
/// no coefficients.
fn dc(writer: &mut BitWriter<Vec<u8>>, shape: &Shape, at: usize) -> io::Result<()> {
    writer.write_exp_golomb(I_16X16_DC)?;
    // intra_chroma_pred_mode DC, mb_qp_delta.
    writer.write_exp_golomb(0)?;
    writer.write_signed_exp_golomb(0)?;

    // nC from the blocks above and to the left (9.2.1): the I_PCM
    // macroblock counts as 16 coefficients, every other one as none.
    let x = at % shape.width;
    let y = at / shape.width;
    let coefficients = |x: usize, y: usize| if x == 0 && y == 0 { 16 } else { 0 };
    let left = (x > 0).then(|| coefficients(x - 1, y));
    let above = (y > 0).then(|| coefficients(x, y - 1));
    let nc = match (left, above) {
        (Some(left), Some(above)) => (left + above + 1) >> 1,
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => 0,
    };

    // coeff_token for TotalCoeff 0 and TrailingOnes 0 (Table 9-5).
    match nc {
        0..2 => writer.write_bits(0b1, 1),
        2..4 => writer.write_bits(0b11, 2),
        4..8 => writer.write_bits(0b1111, 4),
        _ => writer.write_bits(0b000011, 6),
    }
}

/// rbsp_stop_one_bit, then zeros to the byte.
fn trailing(writer: &mut BitWriter<Vec<u8>>) -> io::Result<()> {
    writer.write_bit(true)?;
    writer.align()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SPS an iPhone on iOS 27.3 sends with its screen, as sps.rs has it.
    const SPS: [u8; 17] = [
        0x27, 0x64, 0x00, 0x33, 0x4b, 0x04, 0xc5, 0x14, 0x05, 0x10, 0x16, 0x1a, 0x6e, 0x04, 0x04,
        0x04, 0x04,
    ];

    #[test]
    fn the_black_frame_is_a_pps_then_an_idr_slice() {
        let frame = frame(&SPS, 1).unwrap();

        assert_eq!(&frame[..5], [0, 0, 0, 1, PPS_HEADER]);

        let slice = frame
            .windows(5)
            .position(|at| at == [0, 0, 0, 1, IDR_HEADER])
            .unwrap();

        // The I_PCM macroblock's 384 bytes and about a byte for each other.
        assert!(frame.len() - slice > 384);
        assert!(frame.len() < 32 * 1024);
    }
}
