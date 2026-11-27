use std::io::Cursor;

use anyhow::{Result, ensure};
use protocol::{App, Device, Variant, mention};

use crate::app::abis_of;

/// An app without native libraries runs everywhere.
pub(super) fn abis(apk: &[u8]) -> Vec<String> {
    let Ok(archive) = zip::ZipArchive::new(Cursor::new(apk)) else {
        return Vec::new();
    };

    let mut abis: Vec<String> = archive
        .file_names()
        .filter_map(|name| name.strip_prefix("lib/")?.split_once('/'))
        .map(|(abi, _)| abi.to_owned())
        .filter(|abi| crate::name::valid_name(abi))
        .collect();
    abis.sort();
    abis.dedup();

    abis
}

pub(super) fn called(app: &App) -> &str {
    app.name.as_deref().unwrap_or(&app.id)
}

pub(super) fn fits(device: &Device, apk: &[u8]) -> Result<()> {
    ensure!(
        device.ready(),
        "install failed: {} not ready",
        mention(&device.id)
    );

    let runs = abis_of(device)?;
    let built = abis(apk);
    ensure!(
        built.is_empty() || built.iter().any(|abi| runs.contains(abi)),
        "ABI mismatch: app is built for {}, device runs {}",
        built.join(", "),
        runs.join(", ")
    );

    Ok(())
}

pub(super) fn built_for(abis: &[String]) -> String {
    if abis.is_empty() {
        "any device".to_owned()
    } else {
        abis.join(", ")
    }
}

/// The new build takes over its ABIs from older ones, and an older build with
/// none left goes. A build for any ABI replaces all earlier ones, and stays
/// beside a later build for specific ABIs to serve the devices that one does
/// not.
pub(super) fn supersede(variants: &mut Vec<Variant>, new: &Variant) -> Vec<Variant> {
    if new.abis.is_empty() {
        return std::mem::take(variants);
    }

    let mut gone = Vec::new();

    variants.retain_mut(|old| {
        if old.abis.is_empty() {
            return true;
        }

        let mut left = old.clone();
        left.abis.retain(|abi| !new.abis.contains(abi));

        if left.abis.is_empty() {
            gone.push(left);

            return false;
        }

        *old = left;

        true
    });

    gone
}

#[cfg(test)]
pub(super) mod tests {
    use std::io::Write;

    use protocol::{At, Condition, Described, DeviceId, DeviceState, InstanceId, Means, Way};
    use zip::write::SimpleFileOptions;

    use super::*;

    pub(in crate::apps) fn apk(files: &[&str]) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));

        for file in files {
            archive
                .start_file(*file, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(b"x").unwrap();
        }

        archive.finish().unwrap().into_inner()
    }

    fn device(abis: &[&str]) -> Device {
        let here = At::Instance(InstanceId("SHA256:here".to_owned()));

        Device {
            id: DeviceId::Emulator {
                port: 5554,
                instance: InstanceId("SHA256:here".to_owned()),
            },
            name: "pixel".to_owned(),
            condition: Condition::Connected {
                way: Way {
                    means: Means::Emulator,
                    at: here,
                },
                state: DeviceState::Ready,
                described: Some(Described {
                    release: "16".to_owned(),
                    abis: abis.iter().map(|abi| (*abi).to_owned()).collect(),
                }),
            },
            routes: Vec::new(),
            last: None,
            emulator: None,
        }
    }

    pub(in crate::apps) fn built(build: u32, abis: &[&str]) -> Variant {
        Variant {
            abis: abis.iter().map(|abi| (*abi).to_owned()).collect(),
            build,
            version: None,
            handed_over: 0,
            size: 0,
            hash: build.to_string(),
            file: String::new(),
        }
    }

    pub(in crate::apps) fn app(variants: Vec<Variant>) -> App {
        App {
            id: "com.example".to_owned(),
            name: None,
            icon: None,
            builds: variants.len() as u32,
            variants,
            auto: Vec::new(),
        }
    }

    #[test]
    fn reads_what_an_app_is_built_for() {
        let native = apk(&[
            "AndroidManifest.xml",
            "lib/arm64-v8a/libiad.so",
            "lib/arm64-v8a/libc++.so",
            "lib/x86_64/libiad.so",
            "lib/../up/libbad.so",
        ]);
        assert_eq!(abis(&native), ["arm64-v8a", "x86_64"]);
        assert!(abis(&apk(&["AndroidManifest.xml", "classes.dex"])).is_empty());
        assert!(abis(b"not an archive").is_empty());
    }

    #[test]
    fn gives_a_device_the_variant_built_for_it() {
        let both = app(vec![built(1, &["arm64-v8a"]), built(2, &["x86_64"])]);
        let build = |app: &App, abis: &[&str]| {
            let device = device(abis);

            app.variant(abis_of(&device).unwrap()).map(|it| it.build)
        };

        assert_eq!(build(&both, &["arm64-v8a", "armeabi-v7a"]), Some(1));
        assert_eq!(build(&both, &["x86_64"]), Some(2));
        // An emulator runs both and prefers its own ABI.
        assert_eq!(build(&both, &["x86_64", "arm64-v8a"]), Some(2));
        assert_eq!(build(&both, &["riscv64"]), None);

        let with_any = app(vec![built(1, &[]), built(2, &["arm64-v8a"])]);
        assert_eq!(build(&with_any, &["x86_64"]), Some(1));
        assert_eq!(build(&with_any, &["arm64-v8a"]), Some(2));

        let twice = app(vec![
            built(1, &["arm64-v8a", "x86_64"]),
            built(2, &["x86_64"]),
        ]);
        assert_eq!(build(&twice, &["x86_64"]), Some(2));
        assert_eq!(build(&twice, &["arm64-v8a"]), Some(1));
    }

    #[test]
    fn keeps_a_variant_while_it_is_the_newest_for_something() {
        let builds = |variants: &[Variant]| -> Vec<u32> {
            variants.iter().map(|variant| variant.build).collect()
        };
        let hand = |variants: &mut Vec<Variant>, build: u32, abis: &[&str]| {
            let new = built(build, abis);
            supersede(variants, &new);
            variants.push(new);

            builds(variants)
        };

        // A build for two ABIs keeps the one without a newer build, and goes
        // once both have one.
        let mut variants = vec![built(1, &["armeabi-v7a", "arm64-v8a"])];
        assert_eq!(hand(&mut variants, 2, &["arm64-v8a"]), [1, 2]);
        assert_eq!(variants[0].abis, ["armeabi-v7a"]);
        assert_eq!(hand(&mut variants, 3, &["armeabi-v7a"]), [2, 3]);
        assert_eq!(hand(&mut variants, 4, &["arm64-v8a"]), [3, 4]);
        assert_eq!(hand(&mut variants, 5, &["arm64-v8a", "armeabi-v7a"]), [5]);
        assert_eq!(hand(&mut variants, 6, &["x86_64"]), [5, 6]);
        assert_eq!(hand(&mut variants, 7, &[]), [7]);
        assert_eq!(hand(&mut variants, 8, &["arm64-v8a"]), [7, 8]);
        assert_eq!(hand(&mut variants, 9, &[]), [9]);
    }

    #[test]
    fn refuses_a_package_the_device_cannot_run() {
        let device = device(&["x86_64", "arm64-v8a"]);

        assert!(fits(&device, &apk(&["lib/arm64-v8a/libapp.so"])).is_ok());
        assert!(fits(&device, b"not a package at all").is_ok());

        let refused = fits(&device, &apk(&["lib/armeabi-v7a/libapp.so"]))
            .unwrap_err()
            .to_string();
        assert!(refused.contains("built for armeabi-v7a"), "{refused}");
    }
}
