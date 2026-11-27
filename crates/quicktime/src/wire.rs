//! QuickTime's messages and the CoreMedia values inside them, little-endian
//! throughout. Each message is its length (counting itself) and a magic.
//! Values nest the same way, as a length, a magic and the body. The layout
//! is qvh's (doc/technical_documentation.md), whose captures are the
//! fixtures.

/// A magic as the wire has it: its four letters as a big-endian number,
/// which is written little-endian, so "ping" goes out as "gnip".
pub(crate) const fn magic(letters: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*letters)
}

pub(crate) const PING: u32 = magic(b"ping");
pub(crate) const SYNC: u32 = magic(b"sync");
pub(crate) const ASYN: u32 = magic(b"asyn");
pub(crate) const RPLY: u32 = magic(b"rply");

pub(crate) const CWPA: u32 = magic(b"cwpa");
pub(crate) const AFMT: u32 = magic(b"afmt");
pub(crate) const CVRP: u32 = magic(b"cvrp");
pub(crate) const CLOK: u32 = magic(b"clok");
pub(crate) const TIME: u32 = magic(b"time");
pub(crate) const SKEW: u32 = magic(b"skew");

pub(crate) const FEED: u32 = magic(b"feed");
pub(crate) const EAT: u32 = magic(b"eat!");
pub(crate) const RELS: u32 = magic(b"rels");
const NEED: u32 = magic(b"need");
const HPD1: u32 = magic(b"hpd1");
const HPA1: u32 = magic(b"hpa1");
const HPD0: u32 = magic(b"hpd0");
const HPA0: u32 = magic(b"hpa0");

const KEYV: u32 = magic(b"keyv");
const STRK: u32 = magic(b"strk");
const IDXK: u32 = magic(b"idxk");
const BULV: u32 = magic(b"bulv");
const DICT: u32 = magic(b"dict");
const DATV: u32 = magic(b"datv");
const STRV: u32 = magic(b"strv");
const NMBV: u32 = magic(b"nmbv");
const FDSC: u32 = magic(b"fdsc");
const EXTN: u32 = magic(b"extn");
const SBUF: u32 = magic(b"sbuf");
const OPTS: u32 = magic(b"opts");
const STIA: u32 = magic(b"stia");
const SDAT: u32 = magic(b"sdat");
const LPCM: u32 = magic(b"lpcm");

/// The clock reference that names no clock.
const NO_CLOCK: u64 = 1;

/// The sample description extensions in a format description, and the
/// `avcC` record among them (CMFormatDescription's extension keys).
const ATOMS: u16 = 49;
const AVCC: u16 = 105;

pub(crate) fn letters(magic: u32) -> String {
    String::from_utf8_lossy(&magic.to_be_bytes()).into_owned()
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

#[derive(Debug, PartialEq)]
pub(crate) struct Message<'a> {
    pub kind: u32,
    pub clock: u64,
    pub what: u32,
    /// A SYNC's starts with the id its reply carries.
    pub rest: &'a [u8],
}

impl<'a> Message<'a> {
    pub(crate) fn parse(data: &'a [u8]) -> Option<Self> {
        let kind = u32_at(data, 0)?;

        if kind == PING {
            return Some(Self {
                kind,
                clock: 0,
                what: 0,
                rest: &[],
            });
        }

        Some(Self {
            kind,
            clock: u64_at(data, 4)?,
            what: u32_at(data, 12)?,
            rest: data.get(16..)?,
        })
    }

    pub(crate) fn correlation(&self) -> Option<u64> {
        u64_at(self.rest, 0)
    }

    /// The clock a SYNC names after its id: the phone's, in CWPA and CVRP.
    pub(crate) fn named_clock(&self) -> Option<u64> {
        u64_at(self.rest, 8)
    }
}

fn wrapped(magic: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);

    out.extend(((body.len() + 8) as u32).to_le_bytes());
    out.extend(magic.to_le_bytes());
    out.extend(body);
    out
}

pub(crate) fn ping() -> Vec<u8> {
    let mut body = 0u32.to_le_bytes().to_vec();

    body.extend(1u32.to_le_bytes());
    wrapped(PING, &body)
}

fn reply(correlation: u64, payload: &[u8]) -> Vec<u8> {
    let mut body = correlation.to_le_bytes().to_vec();

    body.extend(0u32.to_le_bytes());
    body.extend(payload);
    wrapped(RPLY, &body)
}

pub(crate) fn reply_clock(correlation: u64, clock: u64) -> Vec<u8> {
    reply(correlation, &clock.to_le_bytes())
}

/// The time goes as a valid CMTime in epoch 0.
pub(crate) fn reply_time(correlation: u64, nanoseconds: u64) -> Vec<u8> {
    let mut time = nanoseconds.to_le_bytes().to_vec();

    time.extend(1_000_000_000u32.to_le_bytes());
    time.extend(1u32.to_le_bytes());
    time.extend(0u64.to_le_bytes());
    reply(correlation, &time)
}

pub(crate) fn reply_skew(correlation: u64, skew: f64) -> Vec<u8> {
    reply(correlation, &skew.to_bits().to_le_bytes())
}

pub(crate) fn reply_empty(correlation: u64) -> Vec<u8> {
    reply(correlation, &0u32.to_le_bytes())
}

pub(crate) fn reply_format(correlation: u64) -> Vec<u8> {
    reply(correlation, &dict(&[("Error", Value::U32(0))]))
}

fn asyn(clock: u64, what: u32, payload: &[u8]) -> Vec<u8> {
    let mut body = clock.to_le_bytes().to_vec();

    body.extend(what.to_le_bytes());
    body.extend(payload);
    wrapped(ASYN, &body)
}

pub(crate) fn need(clock: u64) -> Vec<u8> {
    asyn(clock, NEED, &[])
}

/// This host as a display: QuickTime's code name for the feature, HEVC
/// decoding, and a display size, which the phone does not scale to.
pub(crate) fn hpd1() -> Vec<u8> {
    let size = Value::Dict(vec![
        ("Width", Value::F64(1920.0)),
        ("Height", Value::F64(1200.0)),
    ]);

    asyn(
        NO_CLOCK,
        HPD1,
        &dict(&[
            ("Valeria", Value::Bool(true)),
            ("HEVCDecoderSupports444", Value::Bool(true)),
            ("DisplaySize", size),
        ]),
    )
}

pub(crate) fn hpa1(audio_clock: u64) -> Vec<u8> {
    asyn(
        audio_clock,
        HPA1,
        &dict(&[
            ("BufferAheadInterval", Value::F64(0.073_000_000_000_000_01)),
            ("deviceUID", Value::Str("Valeria")),
            ("ScreenLatency", Value::F64(0.04)),
            ("formats", Value::Data(pcm())),
            ("EDIDAC3Support", Value::U32(0)),
            ("deviceName", Value::Str("Valeria")),
        ]),
    )
}

/// Stops the sound on the phone's audio clock.
pub(crate) fn hpa0(audio_clock: u64) -> Vec<u8> {
    asyn(audio_clock, HPA0, &[])
}

/// Stops the video.
pub(crate) fn hpd0() -> Vec<u8> {
    asyn(NO_CLOCK, HPD0, &[])
}

/// An AudioStreamBasicDescription of 48 kHz linear PCM, two 16-bit channels
/// a frame, then the sample rate twice more as the range it may take.
fn pcm() -> Vec<u8> {
    let mut out = 48_000f64.to_bits().to_le_bytes().to_vec();

    // The format, its flags (signed, packed), the bytes and frames of a
    // packet, the bytes of a frame, channels, bits and the reserved word.
    for word in [LPCM, 12, 4, 1, 4, 2, 16, 0u32] {
        out.extend(word.to_le_bytes());
    }

    out.extend(48_000f64.to_bits().to_le_bytes());
    out.extend(48_000f64.to_bits().to_le_bytes());
    out
}

enum Value {
    Bool(bool),
    Str(&'static str),
    Data(Vec<u8>),
    F64(f64),
    U32(u32),
    Dict(Vec<(&'static str, Value)>),
}

fn value(value: &Value) -> Vec<u8> {
    match value {
        Value::Bool(on) => wrapped(BULV, &[u8::from(*on)]),
        Value::Str(text) => wrapped(STRV, text.as_bytes()),
        Value::Data(data) => wrapped(DATV, data),
        // NSNumber: a type, 6 for a double and 3 for a 32-bit integer.
        Value::F64(number) => wrapped(NMBV, &[&[6], &number.to_bits().to_le_bytes()[..]].concat()),
        Value::U32(number) => wrapped(NMBV, &[&[3], &number.to_le_bytes()[..]].concat()),
        Value::Dict(entries) => dict(entries),
    }
}

fn dict(entries: &[(&str, Value)]) -> Vec<u8> {
    let body: Vec<u8> = entries
        .iter()
        .flat_map(|(key, entry)| {
            wrapped(
                KEYV,
                &[wrapped(STRK, key.as_bytes()), value(entry)].concat(),
            )
        })
        .collect();

    wrapped(DICT, &body)
}

fn values(mut data: &[u8]) -> impl Iterator<Item = (u32, &[u8])> {
    std::iter::from_fn(move || {
        let length = (u32_at(data, 0)? as usize).max(8);
        let magic = u32_at(data, 4)?;
        let body = data.get(8..length)?;

        data = &data[length..];

        Some((magic, body))
    })
}

/// Entries with a text key are passed over.
fn indexed(body: &[u8]) -> impl Iterator<Item = (u16, u32, &[u8])> {
    values(body)
        .filter(|(magic, _)| *magic == KEYV)
        .filter_map(|(_, entry)| {
            let mut parts = values(entry);
            let (key_magic, key) = parts.next()?;
            let (magic, value) = parts.next()?;

            (key_magic == IDXK).then_some(())?;

            Some((
                u16::from_le_bytes(key.get(..2)?.try_into().ok()?),
                magic,
                value,
            ))
        })
}

fn avcc_of(format: &[u8]) -> Option<&[u8]> {
    let (_, extensions) = values(format).find(|(magic, _)| *magic == EXTN)?;
    let (_, _, atoms) =
        indexed(extensions).find(|(key, magic, _)| (*key, *magic) == (ATOMS, DICT))?;
    let (_, _, avcc) = indexed(atoms).find(|(key, magic, _)| (*key, *magic) == (AVCC, DATV))?;

    Some(avcc)
}

/// CVRP's dictionary follows the id and the phone's clock.
pub(crate) fn cvrp_avcc<'a>(message: &Message<'a>) -> Option<&'a [u8]> {
    let (magic, body) = values(message.rest.get(16..)?).next()?;

    (magic == DICT).then_some(())?;

    values(body)
        .filter(|(magic, _)| *magic == KEYV)
        .filter_map(|(_, entry)| values(entry).nth(1))
        .find(|(magic, _)| *magic == FDSC)
        .and_then(|(_, format)| avcc_of(format))
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Sample<'a> {
    /// The time it is shown, in seconds on the phone's clock.
    pub(crate) time: Option<f64>,
    pub(crate) data: &'a [u8],
    pub(crate) avcc: Option<&'a [u8]>,
}

/// A CMTime in seconds. Its value is signed: the first frame of a session
/// came a few milliseconds before zero (iOS 27.3).
fn time(data: &[u8]) -> Option<f64> {
    let value = u64_at(data, 0)?.cast_signed();
    let scale = u32_at(data, 8)?;

    (scale != 0).then(|| value as f64 / f64::from(scale))
}

pub(crate) fn sample<'a>(message: &Message<'a>) -> Option<Sample<'a>> {
    let (magic, buffer) = values(message.rest).next()?;

    (magic == SBUF).then_some(())?;

    let mut sample = Sample::default();
    let mut presented = None;

    for (magic, body) in values(buffer) {
        match magic {
            OPTS => presented = time(body),
            // Its timings: a duration, a presentation and a decode time
            // for each sample. The first presentation is taken.
            STIA => sample.time = body.get(24..48).and_then(time),
            SDAT => sample.data = body,
            FDSC => sample.avcc = avcc_of(body),
            _ => {}
        }
    }

    sample.time = sample.time.or(presented);

    Some(sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A capture from qvh, which keeps each message's length.
    pub(crate) fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn received(name: &str) -> Vec<u8> {
        fixture(name)[4..].to_vec()
    }

    #[test]
    fn what_this_host_says_matches_quicktime() {
        assert_eq!(hpd1(), fixture("asyn-hpd1"));
        assert_eq!(hpa1(0x1_1453_92f0), fixture("asyn-hpa1"));
        assert_eq!(need(0x1_02c1_6ca0), fixture("asyn-need"));
    }

    #[test]
    fn replies_carry_the_requests_id() {
        let data = received("cwpa-request1");
        let cwpa = Message::parse(&data).unwrap();

        assert_eq!((cwpa.kind, cwpa.what), (SYNC, CWPA));
        assert_eq!(cwpa.correlation(), Some(0x1_1357_3de0));
        assert_eq!(cwpa.named_clock(), Some(0x1_135a_74e0));
        assert_eq!(
            reply_clock(0x1_1357_3de0, 0x7fa6_6ce2_0cb0),
            fixture("cwpa-reply1")
        );

        let data = received("afmt-request");
        let afmt = Message::parse(&data).unwrap();

        assert_eq!(
            reply_format(afmt.correlation().unwrap()),
            fixture("afmt-reply")
        );

        let data = received("time-request1");
        let time = Message::parse(&data).unwrap();

        assert_eq!(time.what, TIME);
        assert_eq!(
            reply_time(time.correlation().unwrap(), 0xba62_c442_e1e1),
            fixture("time-reply1")
        );
    }

    #[test]
    fn cvrp_names_the_video_clock_and_its_format() {
        let data = received("cvrp-request");
        let cvrp = Message::parse(&data).unwrap();

        assert_eq!(cvrp.what, CVRP);
        assert_eq!(cvrp.named_clock(), Some(0x1_1353_8da0));
        assert_eq!(
            reply_clock(cvrp.correlation().unwrap(), 0x7fa6_6cd1_0250),
            fixture("cvrp-reply")
        );

        assert_eq!(&cvrp_avcc(&cvrp).unwrap()[..2], [1, 0x64]);
    }

    #[test]
    fn a_feed_holds_one_frame_with_its_format_and_time() {
        let data = received("asyn-feed");
        let feed = Message::parse(&data).unwrap();

        assert_eq!(
            (feed.kind, feed.clock, feed.what),
            (ASYN, 0x7ffb_5cc3_2f60, FEED)
        );

        let sample = sample(&feed).unwrap();

        // An SEI of 30 bytes then an IDR slice of 90712, each after its
        // length.
        assert_eq!(sample.data.len(), 4 + 30 + 4 + 90_712);
        assert_eq!(&sample.data[..4], 30u32.to_be_bytes());
        assert_eq!(
            sample.avcc.map(|avcc| &avcc[..8]),
            Some(&[1, 0x64, 0x00, 0x33, 0xff, 0xe1, 0x00, 0x11][..])
        );
        assert!(sample.time.is_some_and(|time| time > 95_000.0));
    }

    #[test]
    fn magics_read_as_their_letters() {
        assert_eq!(letters(FEED), "feed");
        assert_eq!(ping()[4..8], *b"gnip");
    }
}
