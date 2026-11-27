//! QuickTime's sample buffers carry H.264 in its ISO 14496-15 form: the
//! parameter sets in an `avcC` record, and each sample a run of units, each
//! after its 4-byte length.

use crate::{black, sps};

const START: [u8; 4] = [0, 0, 0, 1];

#[derive(Default)]
pub(crate) struct Avcc {
    parameters: Vec<u8>,
    /// The first SPS as a NAL unit, which a black frame is made for.
    sps: Option<Vec<u8>>,
    codec: Option<String>,
    /// The black frames made, which tells their IDRs apart.
    blacks: u16,
}

impl Avcc {
    /// False if the record cannot be read, which changes nothing.
    pub(crate) fn describe(&mut self, avcc: &[u8]) -> bool {
        let mut reader = std::io::Cursor::new(bytes::Bytes::copy_from_slice(avcc));
        let Ok(record) = scuffle_h264::AVCDecoderConfigurationRecord::parse(&mut reader) else {
            return false;
        };
        let mut parameters = Vec::new();
        let mut codec = None;
        let mut first = None;

        for unit in record.sps.iter().filter(|unit| !unit.is_empty()) {
            let unit = sps::prepared(unit).unwrap_or_else(|| unit.to_vec());

            codec = codec.or_else(|| self::codec(&unit));
            parameters.extend(START);
            parameters.extend(&unit);
            first.get_or_insert(unit);
        }

        for unit in record.pps.iter().filter(|unit| !unit.is_empty()) {
            parameters.extend(START);
            parameters.extend(unit.iter());
        }

        self.parameters = parameters;
        self.sps = first.or(self.sps.take());
        self.codec = codec.or(self.codec.take());

        true
    }

    pub(crate) fn black(&mut self) -> Option<Vec<u8>> {
        self.blacks = self.blacks.wrapping_add(1);

        let frame = black::frame(self.sps.as_ref()?, self.blacks)?;

        Some([self.parameters.as_slice(), &frame].concat())
    }

    pub(crate) fn codec(&self) -> Option<&str> {
        self.codec.as_deref()
    }

    pub(crate) fn frame(&self, sample: &[u8]) -> Option<(bool, Vec<u8>)> {
        if self.parameters.is_empty() {
            return None;
        }

        let mut data = Vec::with_capacity(sample.len() + self.parameters.len());
        let mut key = false;
        let mut rest = sample;

        while !rest.is_empty() {
            let size = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
            let unit = rest.get(4..4 + size)?;

            if let Some(&header) = unit.first() {
                key |= header & 0x1f == 5;
                data.extend(START);
                data.extend(unit);
            }

            rest = &rest[4 + size..];
        }

        if data.is_empty() {
            return None;
        }

        if key {
            data.splice(0..0, self.parameters.iter().copied());
        }

        Some((key, data))
    }
}

fn codec(sps: &[u8]) -> Option<String> {
    let sps = scuffle_h264::Sps::parse_with_emulation_prevention(sps).ok()?;
    // constraint_set0_flag to constraint_set5_flag, then two reserved bits.
    let compatibility = [
        sps.constraint_set0_flag,
        sps.constraint_set1_flag,
        sps.constraint_set2_flag,
        sps.constraint_set3_flag,
        sps.constraint_set4_flag,
        sps.constraint_set5_flag,
    ]
    .into_iter()
    .enumerate()
    .fold(0u8, |byte, (at, on)| byte | u8::from(on) << (7 - at));

    Some(format!(
        "avc1.{:02x}{compatibility:02x}{:02x}",
        sps.profile_idc, sps.level_idc
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SPS an iPhone on iOS 27.3 sends with its screen, as sps.rs has it.
    const SPS: [u8; 17] = [
        0x27, 0x64, 0x00, 0x33, 0x4b, 0x04, 0xc5, 0x14, 0x05, 0x10, 0x16, 0x1a, 0x6e, 0x04, 0x04,
        0x04, 0x04,
    ];
    const PPS: [u8; 4] = [0x28, 0x4a, 0xe3, 0xcb];

    fn record() -> Vec<u8> {
        let mut record = vec![1, 0x64, 0x00, 0x33, 0xff, 0xe1, 0, SPS.len() as u8];

        record.extend(SPS);
        record.extend([1, 0, PPS.len() as u8]);
        record.extend(PPS);
        record
    }

    fn sample(units: &[&[u8]]) -> Vec<u8> {
        units
            .iter()
            .flat_map(|unit| {
                (unit.len() as u32)
                    .to_be_bytes()
                    .into_iter()
                    .chain(unit.iter().copied())
            })
            .collect()
    }

    fn annex_b(units: &[&[u8]]) -> Vec<u8> {
        units
            .iter()
            .flat_map(|unit| START.into_iter().chain(unit.iter().copied()))
            .collect()
    }

    #[test]
    fn samples_become_frames_with_the_parameter_sets_on_key_frames() {
        let mut avcc = Avcc::default();

        assert_eq!(avcc.frame(&sample(&[&[0x25, 1]])), None);
        assert!(avcc.describe(&record()));
        assert_eq!(avcc.codec(), Some("avc1.640033"));

        let (key, data) = avcc.frame(&sample(&[&[0x06, 9], &[0x25, 1, 2]])).unwrap();

        assert!(key);
        assert_eq!(
            data,
            annex_b(&[
                &sps::prepared(&SPS).unwrap(),
                &PPS,
                &[0x06, 9],
                &[0x25, 1, 2]
            ])
        );
        assert_eq!(
            avcc.frame(&sample(&[&[0x21, 3]])),
            Some((false, annex_b(&[&[0x21, 3]])))
        );
    }

    #[test]
    fn a_cut_sample_gives_nothing() {
        let mut avcc = Avcc::default();

        avcc.describe(&record());

        assert_eq!(avcc.frame(&[0, 0, 0, 9, 0x25]), None);
    }
}
