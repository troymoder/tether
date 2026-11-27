use rkyv::{Archive, Deserialize, Serialize};

use crate::DeviceId;

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct App {
    pub id: String,
    pub name: Option<String>,
    /// A file name in tetherd's apps folder.
    pub icon: Option<String>,
    pub builds: u32,
    /// The latest build for each architecture. A build keeps only the
    /// architectures no later build covers.
    pub variants: Vec<Variant>,
    /// The devices each new build is installed on while they are connected,
    /// and reopened on if it was in front. tetherd does this for its own
    /// devices, and the page for the phones on its own WebUSB.
    pub auto: Vec<DeviceId>,
}

impl App {
    pub fn auto_on(&self, device: &DeviceId) -> bool {
        self.auto.contains(device)
    }

    pub fn set_auto(&mut self, device: &DeviceId, on: bool) {
        self.auto.retain(|auto| auto != device);

        if on {
            self.auto.push(device.clone());
        }
    }

    pub fn latest(&self) -> Option<&Variant> {
        self.variants.iter().max_by_key(|variant| variant.build)
    }

    /// `runs` is the device's supported ABIs, preferred first. A build
    /// without native code counts as a build for the device's preferred ABI.
    pub fn build_for(&self, abi: &str, runs: &[String]) -> Option<&Variant> {
        let best = runs.first().is_some_and(|best| best == abi);
        let builds = self.variants.iter().filter(|variant| {
            variant.abis.iter().any(|built| built == abi) || (best && variant.abis.is_empty())
        });

        builds.max_by_key(|variant| variant.build)
    }

    /// The first ABI in the device's own order that has a build.
    pub fn target<'a>(&self, runs: &'a [String]) -> Option<&'a str> {
        runs.iter()
            .find(|abi| self.build_for(abi, runs).is_some())
            .map(String::as_str)
    }

    pub fn variant(&self, runs: &[String]) -> Option<&Variant> {
        self.build_for(self.target(runs)?, runs)
    }
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Variant {
    /// The ABIs it has native code for, as Android names them (`arm64-v8a`).
    /// Empty for a build without native code, which runs on every device.
    pub abis: Vec<String>,
    /// Counts from 1.
    pub build: u32,
    /// None when it has none, or when no Android SDK was available to read
    /// it.
    pub version: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub handed_over: u64,
    pub size: u64,
    /// The SHA-256 of its file, in hex. A device has this build installed
    /// when its copy of the app has the same hash.
    pub hash: String,
    /// A file name in tetherd's apps folder.
    pub file: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_a_device_the_newest_build_for_its_architecture() {
        let build = |build: u32, abis: &[&str]| Variant {
            abis: abis.iter().map(|abi| (*abi).to_owned()).collect(),
            build,
            version: None,
            handed_over: 0,
            size: 0,
            hash: String::new(),
            file: String::new(),
        };

        let mut app = App {
            id: "com.example".to_owned(),
            name: None,
            icon: None,
            builds: 4,
            variants: vec![
                build(1, &["armeabi-v7a"]),
                build(2, &[]),
                build(3, &["x86_64"]),
            ],
            auto: Vec::new(),
        };

        let runs =
            |abis: &[&str]| -> Vec<String> { abis.iter().map(|abi| (*abi).to_owned()).collect() };
        let got = |app: &App, runs: &[String]| app.variant(runs).map(|variant| variant.build);

        let emulator = runs(&["x86_64", "arm64-v8a"]);

        assert_eq!(app.target(&emulator), Some("x86_64"));
        assert_eq!(got(&app, &emulator), Some(3));
        assert_eq!(app.latest().map(|variant| variant.build), Some(3));

        let phone = runs(&["arm64-v8a", "armeabi-v7a"]);

        assert_eq!(got(&app, &phone), Some(2));

        app.variants.retain(|variant| !variant.abis.is_empty());
        assert_eq!(got(&app, &phone), Some(1));
        assert_eq!(app.target(&phone), Some("armeabi-v7a"));
        assert_eq!(got(&app, &runs(&["riscv64"])), None);
        assert_eq!(got(&app, &[]), None);

        app.variants = vec![build(5, &["arm64-v8a", "armeabi-v7a", "x86_64"])];
        assert_eq!(app.target(&phone), Some("arm64-v8a"));
        assert_eq!(got(&app, &phone), Some(5));

        app.variants[0].abis.retain(|abi| abi != "armeabi-v7a");
        app.variants.push(build(6, &["armeabi-v7a"]));
        assert_eq!(app.target(&phone), Some("arm64-v8a"));
        assert_eq!(got(&app, &phone), Some(5));
    }

    #[test]
    fn keeps_the_devices_that_auto_update() {
        let mut app = App {
            id: "com.example".to_owned(),
            name: None,
            icon: None,
            builds: 0,
            variants: Vec::new(),
            auto: Vec::new(),
        };
        let handset = DeviceId::Android("phone".to_owned());

        app.set_auto(&handset, true);
        app.set_auto(&handset, true);
        assert!(app.auto_on(&handset));
        assert_eq!(app.auto.len(), 1);
        app.set_auto(&handset, false);
        assert!(!app.auto_on(&handset));
        assert!(app.auto.is_empty());
    }
}
