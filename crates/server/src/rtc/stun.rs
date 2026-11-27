//! A browser behind another NAT reaches the UDP sockets at the addresses a STUN
//! server sees them at, once each side has sent to the other (UDP hole
//! punching).

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Most NATs close an idle mapping after half a minute or more, so a request
/// this often keeps it open.
pub(super) const STUN_EVERY: Duration = Duration::from_secs(25);

/// STUN's magic cookie (RFC 5389), which every message has after its type
/// and length.
const COOKIE: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];

#[derive(Default)]
pub(super) struct Outside {
    /// By transaction, each with the socket it went from.
    pub(super) asked: Mutex<HashMap<[u8; 12], SocketAddr>>,
    /// By socket: the address it is seen as from outside, and when that was
    /// heard.
    pub(super) seen: Mutex<HashMap<SocketAddr, (SocketAddr, Instant)>>,
}

impl Outside {
    pub(super) fn heard(&self, data: &[u8]) -> bool {
        let Some((id, seen)) = answer(data) else {
            return false;
        };

        let Some(socket) = self.asked.lock().unwrap().remove(&id) else {
            return false;
        };

        let before = self
            .seen
            .lock()
            .unwrap()
            .insert(socket, (seen, Instant::now()));

        // An address seen as itself has no NAT in front of it.
        if seen != socket && before.is_none_or(|(was, _)| was != seen) {
            tracing::info!("stun: {socket} is seen from outside as {seen}");
        }

        true
    }
}

/// A binding request for a transaction: a header with nothing after it.
pub(super) fn request(id: [u8; 12]) -> [u8; 20] {
    let mut request = [0; 20];

    request[..2].copy_from_slice(&[0x00, 0x01]);
    request[4..8].copy_from_slice(&COOKIE);
    request[8..].copy_from_slice(&id);

    request
}

/// Reads a binding success answer: its transaction, and the address it says
/// the request came from (XOR-MAPPED-ADDRESS).
fn answer(data: &[u8]) -> Option<([u8; 12], SocketAddr)> {
    if data.len() < 20 || data[..2] != [0x01, 0x01] || data[4..8] != COOKIE {
        return None;
    }

    let id: [u8; 12] = data[8..20].try_into().ok()?;
    let length = usize::from(u16::from_be_bytes([data[2], data[3]]));
    let mut attributes = data.get(20..20 + length)?;

    while attributes.len() >= 4 {
        let kind = u16::from_be_bytes([attributes[0], attributes[1]]);
        let size = usize::from(u16::from_be_bytes([attributes[2], attributes[3]]));
        let value = attributes.get(4..4 + size)?;

        if kind == 0x0020 && size >= 8 {
            let port = u16::from_be_bytes([value[2] ^ COOKIE[0], value[3] ^ COOKIE[1]]);
            let mut mask = [0; 16];

            mask[..4].copy_from_slice(&COOKIE);
            mask[4..].copy_from_slice(&id);

            let unmasked = |at: usize| value[4 + at] ^ mask[at];
            let ip = match (value[1], size) {
                (1, 8) => IpAddr::from(std::array::from_fn::<u8, 4, _>(unmasked)),
                (2, 20) => IpAddr::from(std::array::from_fn::<u8, 16, _>(unmasked)),
                _ => return None,
            };

            return Some((id, SocketAddr::new(ip, port)));
        }

        // Attributes are padded to four bytes.
        attributes = attributes
            .get(4 + size.next_multiple_of(4)..)
            .unwrap_or_default();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_only_the_answers_it_asked_for() {
        let outside = Outside::default();
        let socket: SocketAddr = "192.168.1.5:5176".parse().unwrap();
        let seen: SocketAddr = "203.0.113.7:40123".parse().unwrap();
        // Shaped as a STUN server answers: a SOFTWARE attribute to skip, with
        // padding, then the address with the cookie over it.
        let answer = |id: [u8; 12]| {
            let mut bytes = vec![0x01, 0x01, 0, 20];

            bytes.extend(COOKIE);
            bytes.extend(id);
            bytes.extend([0x80, 0x22, 0, 3, b'a', b'b', b'c', 0]);
            bytes.extend([0, 0x20, 0, 8, 0, 1]);
            bytes.extend((40123u16 ^ 0x2112).to_be_bytes());
            bytes.extend(
                [203, 0, 113, 7]
                    .iter()
                    .zip(COOKIE)
                    .map(|(byte, mask)| byte ^ mask),
            );

            bytes
        };

        let asked = [7; 12];

        outside.asked.lock().unwrap().insert(asked, socket);

        // Another transaction's answer, and anything that is not STUN, go on
        // to the connections.
        assert!(!outside.heard(&answer([8; 12])));
        assert!(!outside.heard(b"not stun"));
        assert!(outside.seen.lock().unwrap().is_empty());

        assert!(outside.heard(&answer(asked)));
        assert_eq!(outside.seen.lock().unwrap()[&socket].0, seen);
        assert!(outside.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn reads_an_ipv6_address_and_asks_with_a_bare_header() {
        let id = [3; 12];
        let seen: std::net::Ipv6Addr = "2001:db8::1".parse().unwrap();
        let mut mask = COOKIE.to_vec();
        let mut bytes = vec![0x01, 0x01, 0, 24];

        mask.extend(id);
        bytes.extend(COOKIE);
        bytes.extend(id);
        bytes.extend([0, 0x20, 0, 20, 0, 2]);
        bytes.extend((443u16 ^ 0x2112).to_be_bytes());
        bytes.extend(
            seen.octets()
                .iter()
                .zip(mask)
                .map(|(byte, mask)| byte ^ mask),
        );

        assert_eq!(
            answer(&bytes),
            Some((id, SocketAddr::new(seen.into(), 443)))
        );

        assert_eq!(request(id)[..8], [0, 1, 0, 0, 0x21, 0x12, 0xA4, 0x42]);
    }
}
