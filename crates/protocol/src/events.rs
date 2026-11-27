use rkyv::{Archive, Deserialize, Serialize};

use crate::DeviceId;

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Event {
    /// Increases in the order events were recorded.
    pub id: u64,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    pub failed: bool,
    pub what: String,
    /// May name devices with [`mention`], which the page replaces with their
    /// current names when it shows the message ([`mentioned`]).
    pub message: String,
    pub device: Option<DeviceId>,
    /// The host name of the tetherd that did it, or the name of the page
    /// that did it itself.
    pub place: String,
}

/// A field that is None does not filter.
#[derive(Clone, Debug, Default, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct EventQuery {
    pub search: Option<String>,
    pub failed: Option<bool>,
    pub what: Option<String>,
    pub device: Option<DeviceId>,
    pub place: Option<String>,
    pub oldest_first: bool,
    /// The most events to return, from the start of the chosen order.
    pub most: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct EventPage {
    pub events: Vec<Event>,
    pub more: bool,
    /// The distinct values of `what`, `device` and `place` across all kept
    /// events, sorted.
    pub whats: Vec<String>,
    pub devices: Vec<DeviceId>,
    pub places: Vec<String>,
}

/// The page shows the device's current name in place of this, so renaming a
/// device updates every message.
pub fn mention(id: &DeviceId) -> String {
    format!("{{device:{id}}}")
}

/// A mention that is not a valid device id is left as it is.
pub fn mentioned(message: &str, name: impl Fn(&DeviceId) -> String) -> String {
    let mut shown = String::with_capacity(message.len());
    let mut rest = message;

    while let Some(start) = rest.find("{device:") {
        let Some(end) = rest[start..].find('}') else {
            break;
        };

        let token = &rest[start..start + end + 1];
        let id = token["{device:".len()..token.len() - 1].parse::<DeviceId>();

        shown.push_str(&rest[..start]);

        match id {
            Ok(id) => shown.push_str(&name(&id)),
            Err(_) => shown.push_str(token),
        }

        rest = &rest[start + end + 1..];
    }

    shown.push_str(rest);
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InstanceId;

    #[test]
    fn a_mentioned_device_is_shown_by_its_name() {
        let phone = DeviceId::Android("AAA".to_owned());
        let emulator = DeviceId::Emulator {
            port: 5554,
            instance: InstanceId("SHA256:a/b".to_owned()),
        };
        let message = format!(
            "Connected to {}, not {}",
            mention(&phone),
            mention(&emulator)
        );
        let name = |id: &DeviceId| {
            if *id == phone {
                "Phone".to_owned()
            } else {
                id.to_string()
            }
        };

        assert_eq!(
            mentioned(&message, name),
            "Connected to Phone, not emulator:5554@SHA256:a/b"
        );
        assert_eq!(mentioned("plain {text", name), "plain {text");
        assert_eq!(mentioned("{device:what}", name), "{device:what}");
    }

    #[test]
    fn a_device_id_reads_back_as_it_was_written() {
        let ids = [
            DeviceId::Iphone("0000813000123D083E8B803A".to_owned()),
            DeviceId::Android("39181FDJG005SD".to_owned()),
            DeviceId::Emulator {
                port: 5556,
                instance: InstanceId("SHA256:x+y/z".to_owned()),
            },
        ];

        for id in ids {
            assert_eq!(id.to_string().parse::<DeviceId>(), Ok(id));
        }

        assert!("pixel".parse::<DeviceId>().is_err());
        assert!("emulator:x@SHA256:a".parse::<DeviceId>().is_err());
        assert!("emulator:5554".parse::<DeviceId>().is_err());
    }
}
