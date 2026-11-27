//! The wire format is described at the top of crates/android/helper/Main.kt.
//! The helper answers each request on its own thread, so replies can arrive
//! out of order.

use std::fmt::Debug;

use adb::libadb::Session;
use adb::{Device, Process};
use anyhow::{Context, Result, bail, ensure};

const DEX: &str = "/data/local/tmp/tether-helper.dex";

/// Numbered as in Main.kt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Apps = 1,
    Thumbnails = 2,
    Count = 3,
    /// The arguments are offset, limit, then folders.
    Page = 4,
    /// The arguments are path, offset, length.
    Read = 5,
}

/// The helper exits when its input closes.
pub async fn start<S>(device: &Device<S>, dex: &[u8]) -> Result<Process<S>>
where
    S: Session + Clone,
    S::IoError: Debug,
{
    // Pushed on every start, because the device may have been wiped since.
    device.push(dex, DEX).await?;

    device
        .spawn(&format!("CLASSPATH={DEX} app_process / tether.Main"), None)
        .await
}

pub fn request(id: u32, kind: Kind, arguments: &[&str]) -> Vec<u8> {
    let mut bytes = Vec::new();

    bytes.extend_from_slice(&id.to_be_bytes());
    bytes.push(kind as u8);
    bytes.extend_from_slice(&(arguments.len() as u32).to_be_bytes());

    for argument in arguments {
        field(&mut bytes, argument.as_bytes());
    }

    bytes
}

fn field(bytes: &mut Vec<u8>, field: &[u8]) {
    bytes.extend_from_slice(&(field.len() as u32).to_be_bytes());
    bytes.extend_from_slice(field);
}

pub type Record = Vec<Vec<u8>>;

#[derive(Debug, PartialEq)]
pub struct Reply {
    pub id: u32,
    pub records: Result<Vec<Record>, String>,
}

#[derive(Default)]
pub struct Replies(Vec<u8>);

impl Replies {
    pub fn feed(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    pub fn whole(&mut self) -> Result<Option<Reply>> {
        let mut reader = Reader(&self.0);
        let Some(reply) = reader.reply()? else {
            return Ok(None);
        };

        let used = self.0.len() - reader.0.len();

        self.0.drain(..used);

        Ok(Some(reply))
    }
}

/// Each read returns None if the buffer ends before the value does.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn number(&mut self) -> Option<u32> {
        let (number, rest) = self.0.split_first_chunk::<4>()?;

        self.0 = rest;

        Some(u32::from_be_bytes(*number))
    }

    fn bytes(&mut self) -> Option<Vec<u8>> {
        let length = self.number()? as usize;
        let (bytes, rest) = self.0.split_at_checked(length)?;

        self.0 = rest;

        Some(bytes.to_vec())
    }

    fn record(&mut self) -> Option<Record> {
        let count = self.number()?;

        (0..count).map(|_| self.bytes()).collect()
    }

    fn reply(&mut self) -> Result<Option<Reply>> {
        let parsed = (|| {
            let id = self.number()?;
            let (status, rest) = self.0.split_first()?;

            self.0 = rest;

            let records = match status {
                0 => {
                    let count = self.number()?;

                    Ok((0..count).map(|_| self.record()).collect::<Option<_>>()?)
                }
                1 => Err(String::from_utf8_lossy(&self.bytes()?).into_owned()),
                other => return Some(Err(*other)),
            };

            Some(Ok(Reply { id, records }))
        })();

        match parsed {
            None => Ok(None),
            Some(Ok(reply)) => Ok(Some(reply)),
            Some(Err(status)) => bail!("helper failed: status {status}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub id: String,
    pub label: Option<String>,
    pub version: Option<String>,
    /// A PNG icon or a JPEG thumbnail, or the reason there is none. None
    /// for an app with no icon of its own.
    pub image: Result<Option<Vec<u8>>, String>,
}

impl Found {
    pub fn read(record: Record) -> Result<Found> {
        let [id, reason, label, version, image] = <[Vec<u8>; 5]>::try_from(record)
            .ok()
            .context("helper failed: bad record")?;
        let text = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned();
        // A record cannot omit a field, so the helper sends an empty one for a
        // missing value.
        let said = |bytes: Vec<u8>| (!bytes.is_empty()).then(|| text(bytes));

        Ok(Found {
            id: text(id),
            label: said(label),
            version: said(version),
            image: match said(reason) {
                Some(reason) => Err(reason),
                None => Ok((!image.is_empty()).then_some(image)),
            },
        })
    }

    pub fn kept(&self) -> Vec<u8> {
        let (reason, image) = match &self.image {
            Ok(image) => (&[][..], image.as_deref().unwrap_or_default()),
            Err(reason) => (reason.as_bytes(), &[][..]),
        };
        let parts = [
            self.id.as_bytes(),
            reason,
            self.label.as_deref().unwrap_or_default().as_bytes(),
            self.version.as_deref().unwrap_or_default().as_bytes(),
            image,
        ];
        let mut bytes = (parts.len() as u32).to_be_bytes().to_vec();

        for part in parts {
            field(&mut bytes, part);
        }

        bytes
    }

    pub fn unkept(bytes: &[u8]) -> Option<Found> {
        let mut reader = Reader(bytes);
        let record = reader.record()?;

        reader.0.is_empty().then_some(())?;

        Found::read(record).ok()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Medium {
    pub path: String,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub changed: u64,
    pub video: bool,
    pub id: u64,
}

impl Medium {
    pub fn read(record: Record) -> Result<Medium> {
        let [path, size, changed, kind, id] = <[Vec<u8>; 5]>::try_from(record)
            .ok()
            .context("helper failed: bad record")?;
        let number = |bytes: Vec<u8>| -> Result<u64> {
            String::from_utf8(bytes)?
                .parse()
                .context("helper failed: bad number")
        };

        Ok(Medium {
            path: String::from_utf8_lossy(&path).into_owned(),
            size: number(size)?,
            changed: number(changed)?,
            // MediaStore's media type: 1 is a photo, and 3 a video.
            video: number(kind)? == 3,
            id: number(id)?,
        })
    }
}

pub fn count(records: Vec<Record>) -> Result<u64> {
    let [record] = <[Record; 1]>::try_from(records)
        .ok()
        .context("helper failed: no count")?;
    let [count] = <[Vec<u8>; 1]>::try_from(record)
        .ok()
        .context("helper failed: no count")?;

    ensure!(!count.is_empty(), "helper failed: no count");

    Ok(String::from_utf8(count)?.parse()?)
}

pub fn slice(records: Vec<Record>) -> Result<Vec<u8>> {
    let [record] = <[Record; 1]>::try_from(records)
        .ok()
        .context("helper failed: no bytes")?;
    let [bytes] = <[Vec<u8>; 1]>::try_from(record)
        .ok()
        .context("helper failed: no bytes")?;

    Ok(bytes)
}

/// MediaStore stores `/sdcard` as `/storage/emulated/0`.
pub fn real(path: &str) -> String {
    match path.strip_prefix("/sdcard/") {
        Some(rest) => format!("/storage/emulated/0/{rest}"),
        None => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(id: u32, records: &[&[&[u8]]]) -> Vec<u8> {
        let mut bytes = id.to_be_bytes().to_vec();

        bytes.push(0);
        bytes.extend_from_slice(&(records.len() as u32).to_be_bytes());

        for record in records {
            bytes.extend_from_slice(&(record.len() as u32).to_be_bytes());

            for part in *record {
                field(&mut bytes, part);
            }
        }

        bytes
    }

    #[test]
    fn writes_a_request() {
        assert_eq!(
            request(7, Kind::Apps, &["a", "bc"]),
            [
                0, 0, 0, 7, 1, 0, 0, 0, 2, 0, 0, 0, 1, b'a', 0, 0, 0, 2, b'b', b'c'
            ]
        );
    }

    #[test]
    fn splits_replies_as_they_come() {
        let app: &[&[u8]] = &[b"com.example", b"", b"Example", b"2.4.0", b"\x89PNG"];
        let broken: &[&[u8]] = &[b"com.gone", b"NameNotFoundException", b"", b"", b""];
        let mut both = sent(3, &[app, broken]);
        let mut failed = 4u32.to_be_bytes().to_vec();

        failed.push(1);
        field(&mut failed, b"no media provider");
        both.extend_from_slice(&failed);

        let mut replies = Replies::default();

        let mut got = Vec::new();

        for byte in &both {
            replies.feed(&[*byte]);

            while let Some(reply) = replies.whole().unwrap() {
                got.push(reply);
            }
        }

        assert_eq!(got.len(), 2);
        assert_eq!(got[1].id, 4);
        assert_eq!(got[1].records, Err("no media provider".to_owned()));

        let records = got.remove(0).records.unwrap();
        let found: Vec<Found> = records
            .into_iter()
            .map(|r| Found::read(r).unwrap())
            .collect();

        assert_eq!(found[0].label.as_deref(), Some("Example"));
        assert_eq!(found[0].version.as_deref(), Some("2.4.0"));
        assert_eq!(found[0].image, Ok(Some(b"\x89PNG".to_vec())));
        assert_eq!(found[1].image, Err("NameNotFoundException".to_owned()));
    }

    #[test]
    fn refuses_a_status_it_does_not_know() {
        let mut replies = Replies::default();

        replies.feed(&[0, 0, 0, 1, 9]);

        assert!(replies.whole().is_err());
    }

    #[test]
    fn keeps_what_was_found() {
        let found = Found {
            id: "com.example".to_owned(),
            label: Some("Example".to_owned()),
            version: Some("2.4.0".to_owned()),
            image: Ok(Some(vec![1, 2, 3])),
        };
        let failed = Found {
            image: Err("no icon".to_owned()),
            ..found.clone()
        };
        let bare = Found {
            label: None,
            version: None,
            image: Ok(None),
            ..found.clone()
        };

        assert_eq!(Found::unkept(&found.kept()), Some(found.clone()));
        assert_eq!(Found::unkept(&failed.kept()), Some(failed));
        assert_eq!(Found::unkept(&bare.kept()), Some(bare));
        assert_eq!(Found::unkept(&found.kept()[..5]), None);
    }

    #[test]
    fn reads_the_gallery() {
        let page = sent(
            1,
            &[&[
                b"/storage/emulated/0/DCIM/Camera/a.mp4",
                b"5120",
                b"1795200000",
                b"3",
                b"16",
            ]],
        );
        let mut replies = Replies::default();

        replies.feed(&page);

        let records = replies.whole().unwrap().unwrap().records.unwrap();
        let medium = Medium::read(records.into_iter().next().unwrap()).unwrap();

        assert_eq!(
            medium,
            Medium {
                path: "/storage/emulated/0/DCIM/Camera/a.mp4".to_owned(),
                size: 5120,
                changed: 1_795_200_000,
                video: true,
                id: 16,
            }
        );

        assert_eq!(count(vec![vec![b"42".to_vec()]]).unwrap(), 42);
        assert!(count(Vec::new()).is_err());
        assert_eq!(real("/sdcard/Movies"), "/storage/emulated/0/Movies");
    }

    #[test]
    fn reads_a_slice() {
        assert_eq!(
            request(2, Kind::Read, &["/a", "5", "3"]),
            [
                0, 0, 0, 2, 5, 0, 0, 0, 3, 0, 0, 0, 2, b'/', b'a', 0, 0, 0, 1, b'5', 0, 0, 0, 1,
                b'3'
            ]
        );

        let mut replies = Replies::default();

        replies.feed(&sent(2, &[&[b"abc"]]));

        let records = replies.whole().unwrap().unwrap().records.unwrap();

        assert_eq!(slice(records).unwrap(), b"abc");
        assert_eq!(slice(vec![vec![Vec::new()]]).unwrap(), b"");
        assert!(slice(Vec::new()).is_err());
    }
}
