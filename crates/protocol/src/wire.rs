use rkyv::api::high::{HighSerializer, HighValidator, to_bytes_in};
use rkyv::bytecheck::CheckBytes;
use rkyv::de::Pool;
use rkyv::rancor::{Error, Strategy};
use rkyv::ser::allocator::ArenaHandle;
use rkyv::{Archive, Archived, Deserialize, Serialize};

pub const CONTROL: &str = "control";

/// Each [`Media`](crate::Media) transfer gets a DataChannel of its own, so
/// large transfers do not hold up the control channel.
pub const MEDIA: &str = "media";

/// tetherd's first message on the channel is an [`encode`]d
/// `Result<(), String>`, the outcome of opening it. After that raw bytes go
/// each way, at most [`PART`] per message.
pub fn adb_label(device: &str, service: &str) -> String {
    format!("adb\n{device}\n{service}")
}

pub fn adb_of(label: &str) -> Option<(&str, &str)> {
    label.strip_prefix("adb\n")?.split_once('\n')
}

/// The largest DataChannel message every browser accepts.
pub const PART: usize = 64 * 1024;

/// Each part is a byte that is 1 when more parts follow and 0 on the last,
/// then up to [`PART`] bytes.
pub fn parts(message: &[u8]) -> impl Iterator<Item = Vec<u8>> + '_ {
    let count = message.len().div_ceil(PART).max(1);

    (0..count).map(move |at| {
        let piece = &message[(at * PART).min(message.len())..((at + 1) * PART).min(message.len())];
        let more = u8::from(at + 1 < count);

        [&[more][..], piece].concat()
    })
}

#[derive(Default)]
pub struct Gather(Vec<u8>);

impl Gather {
    pub fn add(&mut self, part: &[u8]) -> Option<Vec<u8>> {
        let (&more, piece) = part.split_first()?;

        self.0.extend_from_slice(piece);

        (more == 0).then(|| std::mem::take(&mut self.0))
    }
}

pub fn encode<T>(message: &T) -> Vec<u8>
where
    T: for<'a> Serialize<HighSerializer<Vec<u8>, ArenaHandle<'a>, Error>>,
{
    // Writing to memory cannot fail, and every message type can be
    // serialized.
    to_bytes_in(message, Vec::new()).expect("a message that can be written")
}

/// The message is validated first, because it comes from the other side.
pub fn access<T>(message: &[u8]) -> Result<&Archived<T>, String>
where
    T: Archive,
    T::Archived: for<'a> CheckBytes<HighValidator<'a, Error>>,
{
    rkyv::access(message).map_err(|error: Error| error.to_string())
}

pub fn own<T>(part: &Archived<T>) -> T
where
    T: Archive,
    T::Archived: Deserialize<T, Strategy<Pool, Error>>,
{
    // Only shared pointers can fail to deserialize, and no message has one.
    rkyv::deserialize(part).expect("a message without shared pointers")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Admitted, ArchivedRequest, ArchivedToServer, Request, ToClient, ToServer};

    #[test]
    fn a_message_survives_the_trip() {
        let sent = ToServer::Call {
            id: 7,
            request: Request::HandOver {
                id: "com.example".to_owned(),
                apk: b"the file".to_vec(),
                from: None,
            },
        };

        // Offset by one byte so the message does not start at an aligned address.
        let mut received = vec![0];

        received.extend(encode(&sent));

        let message = access::<ToServer>(&received[1..]).unwrap();

        let ArchivedToServer::Call { id, request, .. } = message else {
            panic!("not a call");
        };

        let ArchivedRequest::HandOver { apk, .. } = request else {
            panic!("not a file");
        };

        assert_eq!(id.to_native(), 7);
        assert_eq!(apk.as_slice(), b"the file");
        assert_eq!(own::<ToServer>(message), sent);
    }

    #[test]
    fn a_broken_message_is_refused() {
        assert!(access::<ToClient>(&[0xff; 24]).is_err());
        assert!(access::<ToClient>(&[]).is_err());

        let mut message = encode(&Admitted::Answer {
            answer: "v=0".to_owned(),
            you: "a print".to_owned(),
        });

        message.truncate(message.len() - 3);
        assert!(access::<Admitted>(&message).is_err());
    }

    #[test]
    fn a_message_is_cut_into_parts_and_put_together() {
        for size in [0, 1, PART - 1, PART, PART + 1, 3 * PART + 7] {
            let message: Vec<u8> = (0..size).map(|at| at as u8).collect();
            let mut gather = Gather::default();
            let parts: Vec<_> = parts(&message).collect();

            assert_eq!(parts.len(), size.div_ceil(PART).max(1));
            assert!(parts.iter().all(|part| part.len() <= PART + 1));

            let (last, rest) = parts.split_last().unwrap();

            for part in rest {
                assert_eq!(gather.add(part), None);
            }

            assert_eq!(gather.add(last), Some(message));
        }
    }

    #[test]
    fn an_adb_label_says_its_device_and_service() {
        let label = adb_label("emulator-5554", "shell,v2,raw:ls");

        assert_eq!(adb_of(&label), Some(("emulator-5554", "shell,v2,raw:ls")));
        assert_eq!(adb_of(CONTROL), None);
    }
}
