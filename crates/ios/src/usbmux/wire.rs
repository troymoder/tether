//! usbmux as it goes over the USB bulk endpoints: packets of a mux header and
//! a payload, every number big endian. TCP connections inside it carry a bare
//! TCP header with no checksum. Written from libimobiledevice's usbmuxd
//! `device.c`, as webmuxd (github.com/hack-different/webmuxd) also does.

pub(super) const VERSION: u32 = 0;
pub(super) const CONTROL: u32 = 1;
pub(super) const SETUP: u32 = 2;
pub(super) const TCP: u32 = 6;

/// The magic of a version 2 header from the host, and from the device.
const SENT: u32 = 0xfeed_face;
pub(super) const RECEIVED: u32 = 0xface_face;

const SHORT: usize = 8;
const LONG: usize = 16;
pub(super) const TCP_HEADER: usize = 20;

/// The largest packet the host hands the device at once, as usbmuxd's
/// `USB_MTU`.
pub(super) const MOST_OUT: usize = 3 * 16384;
const MOST_IN: usize = 1024 * 1024;

pub(super) const SYN: u8 = 0x02;
pub(super) const RST: u8 = 0x04;
pub(super) const ACK: u8 = 0x10;
pub(super) const FIN: u8 = 0x01;

fn header_size(protocol: u32, version: u32) -> usize {
    if protocol == VERSION || version < 2 {
        SHORT
    } else {
        LONG
    }
}

pub(super) fn packet(protocol: u32, payload: &[u8], version: u32, sequence: (u16, u16)) -> Vec<u8> {
    let header = header_size(protocol, version);
    let mut packet = Vec::with_capacity(header + payload.len());
    let length = (header + payload.len()) as u32;

    packet.extend(protocol.to_be_bytes());
    packet.extend(length.to_be_bytes());

    if header == LONG {
        packet.extend(SENT.to_be_bytes());
        packet.extend(sequence.0.to_be_bytes());
        packet.extend(sequence.1.to_be_bytes());
    }

    packet.extend(payload);
    packet
}

#[derive(Debug, PartialEq)]
pub(super) struct Packet {
    pub(super) protocol: u32,
    /// The magic and the sequence number the host sends back in its next
    /// packets (the header's `rx_seq`), under version 2.
    pub(super) magic: Option<(u32, u16)>,
    pub(super) payload: Vec<u8>,
}

/// A USB transfer may hold part of a packet or several back to back. Every
/// header starts with the protocol and the whole length, so gathering works
/// before and after the version is known.
#[derive(Default)]
pub(super) struct Gather {
    held: Vec<u8>,
}

impl Gather {
    pub(super) fn push(&mut self, chunk: &[u8], version: u32) -> Result<Vec<Packet>, String> {
        self.held.extend_from_slice(chunk);

        let mut packets = Vec::new();
        let mut at = 0;

        while self.held.len() - at >= SHORT {
            let word = |from: usize| {
                u32::from_be_bytes([
                    self.held[at + from],
                    self.held[at + from + 1],
                    self.held[at + from + 2],
                    self.held[at + from + 3],
                ])
            };
            let protocol = word(0);
            let length = word(4) as usize;
            let header = header_size(protocol, version);

            if length < header || length > MOST_IN {
                self.held.clear();

                return Err(format!("usbmux failed: packet of {length} bytes"));
            }

            if self.held.len() - at < length {
                break;
            }

            let magic = (header == LONG).then(|| {
                let sequence = u16::from_be_bytes([self.held[at + 14], self.held[at + 15]]);

                (word(8), sequence)
            });

            packets.push(Packet {
                protocol,
                magic,
                payload: self.held[at + header..at + length].to_vec(),
            });
            at += length;
        }

        self.held.drain(..at);

        Ok(packets)
    }
}

/// The payload of a version packet: major, minor and padding.
pub(super) fn version(major: u32) -> Vec<u8> {
    [major, 0, 0]
        .iter()
        .flat_map(|word| word.to_be_bytes())
        .collect()
}

pub(super) fn major(payload: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(payload.get(..4)?.try_into().ok()?))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Segment {
    pub(super) from: u16,
    pub(super) to: u16,
    pub(super) sequence: u32,
    pub(super) acknowledged: u32,
    pub(super) flags: u8,
    /// The window as sent, which counts in steps of 256 bytes.
    pub(super) window: u16,
}

pub(super) fn segment(header: Segment, data: &[u8]) -> Vec<u8> {
    let mut segment = Vec::with_capacity(TCP_HEADER + data.len());

    segment.extend(header.from.to_be_bytes());
    segment.extend(header.to.to_be_bytes());
    segment.extend(header.sequence.to_be_bytes());
    segment.extend(header.acknowledged.to_be_bytes());
    segment.push(((TCP_HEADER / 4) as u8) << 4);
    segment.push(header.flags);
    segment.extend(header.window.to_be_bytes());
    // Checksum and urgent pointer, which usbmux leaves out.
    segment.extend([0; 4]);
    segment.extend(data);
    segment
}

pub(super) fn parse_segment(segment: &[u8]) -> Option<(Segment, &[u8])> {
    if segment.len() < TCP_HEADER {
        return None;
    }

    let short = |at: usize| u16::from_be_bytes([segment[at], segment[at + 1]]);
    let word = |at: usize| u32::from_be_bytes(segment[at..at + 4].try_into().unwrap_or_default());
    let offset = (usize::from(segment[12] >> 4) * 4).max(TCP_HEADER);
    let header = Segment {
        from: short(0),
        to: short(2),
        sequence: word(4),
        acknowledged: word(8),
        flags: segment[13],
        window: short(14),
    };

    Some((header, segment.get(offset..)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_go_out_as_usbmuxd_sends_them() {
        // The version handshake has the short header whatever was negotiated.
        assert_eq!(
            packet(VERSION, &version(2), 0, (0, 0)),
            [0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            packet(SETUP, &[7], 2, (0, 0xffff)),
            [
                0, 0, 0, 2, 0, 0, 0, 17, 0xfe, 0xed, 0xfa, 0xce, 0, 0, 0xff, 0xff, 7
            ]
        );
    }

    #[test]
    fn packets_are_gathered_across_transfers() {
        let mut gather = Gather::default();
        let one = [
            0, 0, 0, 1, 0, 0, 0, 17, 0xfa, 0xce, 0xfa, 0xce, 0, 1, 0, 2, 9,
        ];
        let two = [0, 0, 0, 6, 0, 0, 0, 16, 0xfa, 0xce, 0xfa, 0xce, 0, 2, 0, 3];
        let joined: Vec<u8> = one.iter().chain(&two).copied().collect();

        assert_eq!(gather.push(&joined[..5], 2), Ok(Vec::new()));

        let got = gather.push(&joined[5..], 2).unwrap();

        assert_eq!(got.len(), 2);
        assert_eq!(got[0].payload, [9]);
        assert_eq!(got[0].magic, Some((RECEIVED, 2)));
        assert_eq!(got[1].protocol, TCP);
        assert!(gather.push(&[0, 0, 0, 6, 0, 0, 0, 3], 2).is_err());
    }

    #[test]
    fn a_segment_reads_back_as_written() {
        let header = Segment {
            from: 1,
            to: 62078,
            sequence: 5,
            acknowledged: 9,
            flags: ACK,
            window: 512,
        };
        let written = segment(header, b"hi");

        assert_eq!(written.len(), TCP_HEADER + 2);
        assert_eq!(parse_segment(&written), Some((header, &b"hi"[..])));
        assert_eq!(major(&version(2)), Some(2));
    }
}
