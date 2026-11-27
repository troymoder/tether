//! Crashes are read from Android's drop box, which keeps each one with the
//! process's state and survives the log rotating and reboots.

use adb::Device;
use adb::libadb::Session;
use anyhow::Result;

/// The variants are in the order of their names, so sorting by kind sorts by
/// name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Crash,
    NativeCrash,
    NotResponding,
    Tombstone,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Crash => "Crash",
            Kind::NativeCrash => "Native crash",
            Kind::NotResponding => "Not responding",
            Kind::Tombstone => "Tombstone",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Crash {
    /// Written like `2026-11-20 08:13:50`, which sorts in time order.
    pub when: String,
    pub kind: Kind,
    pub process: Option<String>,
    pub text: String,
}

pub async fn list<S>(device: &Device<S>) -> Result<Vec<Crash>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let listed = device.sh(["dumpsys", "dropbox"]).await?;
    let mut tags: Vec<&str> = listed
        .lines()
        .filter_map(|line| line.split(' ').nth(2))
        .filter(|tag| kind(tag).is_some())
        .collect();

    tags.sort_unstable();
    tags.dedup();

    let mut found = Vec::new();

    // `dumpsys dropbox --print` takes one tag at a time.
    for tag in tags {
        let printed = device.sh(["dumpsys", "dropbox", "--print", tag]).await?;

        found.extend(entries(&printed));
    }

    found.sort_by(|a, b| b.when.cmp(&a.when));

    Ok(without_doubles(found))
}

fn kind(tag: &str) -> Option<Kind> {
    let lower = tag.to_lowercase();

    // The tag is passed to a command on the device, so only plain names are
    // accepted. Proto tags hold binary entries.
    if !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || lower.contains("proto") {
        None
    } else if lower.contains("native_crash") {
        Some(Kind::NativeCrash)
    } else if lower.contains("crash") {
        Some(Kind::Crash)
    } else if lower.contains("anr") {
        Some(Kind::NotResponding)
    } else if lower.contains("tombstone") {
        Some(Kind::Tombstone)
    } else {
        None
    }
}

fn entries(printed: &str) -> Vec<Crash> {
    printed
        .split("========================================\n")
        .skip(1)
        .filter_map(|entry| {
            let (head, text) = entry.split_once('\n')?;
            let mut head = head.splitn(3, ' ');
            let when = format!("{} {}", head.next()?, head.next()?);
            let kind = kind(head.next()?.split(' ').next()?)?;

            // An app crash names its process on a `Process:` line, and a
            // native program's tombstone names it between `>>>` and `<<<`.
            let process = text
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Process: ").or_else(|| {
                        let (_, named) = line.split_once(">>> ")?;

                        named.split(" <<<").next()
                    })
                })
                .map(str::trim)
                .filter(|process| !process.is_empty())
                .map(str::to_owned);

            Some(Crash {
                when,
                kind,
                process,
                text: text.trim().to_owned(),
            })
        })
        .collect()
}

fn seconds(when: &str) -> Option<i64> {
    let mut parts = when
        .get(11..19)?
        .split(':')
        .map(|part| part.parse::<i64>().ok());

    Some(parts.next()?? * 3600 + parts.next()?? * 60 + parts.next()??)
}

/// A native crash is kept twice, as a crash and as its tombstone, at most a
/// second apart. Drops the tombstone of each such pair.
fn without_doubles(found: Vec<Crash>) -> Vec<Crash> {
    let doubled = |tombstone: &Crash| {
        found.iter().any(|crash| {
            crash.kind == Kind::NativeCrash
                && crash.process == tombstone.process
                && crash.when.get(..10) == tombstone.when.get(..10)
                && seconds(&crash.when)
                    .zip(seconds(&tombstone.when))
                    .is_some_and(|(crash, tombstone)| (crash - tombstone).abs() <= 1)
        })
    };

    found
        .iter()
        .filter(|crash| !(crash.kind == Kind::Tombstone && doubled(crash)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_drop_box_prints() {
        let printed = "Drop box contents: 55 entries\nSearching for: system_app_crash\n\n\
            ========================================\n\
            2026-11-20 08:13:50 system_app_crash (text, 1163 bytes)\n\
            SystemUptimeMs: 64054\nProcess: com.android.settings\nPID: 1374\n\n\
            android.app.RemoteServiceException: shell-induced crash\n\tat android.app.ActivityThread\n\
            ========================================\n\
            2026-11-20 09:00:01 SYSTEM_TOMBSTONE (text, 90 bytes)\n\
            pid: 1, tid: 2, name: worker  >>> /system/bin/thing <<<\nsignal 11 (SIGSEGV)\n\
            ========================================\n\
            2026-11-20 09:30:00 system_app_anr (text, 20 bytes)\n\
            Subject: stuck\n";
        let found = entries(printed);

        assert_eq!(found.len(), 3);
        assert_eq!(
            (
                found[0].when.as_str(),
                found[0].kind,
                found[0].process.as_deref()
            ),
            (
                "2026-11-20 08:13:50",
                Kind::Crash,
                Some("com.android.settings")
            )
        );

        assert!(found[0].text.ends_with("at android.app.ActivityThread"));
        assert_eq!(found[1].kind, Kind::Tombstone);
        assert_eq!(found[1].process.as_deref(), Some("/system/bin/thing"));
        assert_eq!(found[2].process, None);

        assert_eq!(kind("data_app_native_crash"), Some(Kind::NativeCrash));
        assert_eq!(kind("data_app_anr"), Some(Kind::NotResponding));
        assert_eq!(kind("SYSTEM_TOMBSTONE"), Some(Kind::Tombstone));
        assert_eq!(kind("SYSTEM_TOMBSTONE_PROTO_WITH_HEADERS"), None);
        assert_eq!(kind("system_server_strictmode"), None);
        assert_eq!(kind("crash;rm"), None);

        let entry = |when: &str, kind: Kind, process: &str| Crash {
            when: when.to_owned(),
            kind,
            process: Some(process.to_owned()),
            text: String::new(),
        };

        let kept = without_doubles(vec![
            entry("2026-11-20 06:05:21", Kind::Tombstone, "bluetooth"),
            entry("2026-11-20 06:05:20", Kind::NativeCrash, "bluetooth"),
            entry("2026-11-20 06:05:20", Kind::Tombstone, "app_process"),
            entry("2026-11-20 05:00:00", Kind::Tombstone, "bluetooth"),
        ]);

        assert_eq!(
            kept.iter().map(|crash| crash.kind).collect::<Vec<_>>(),
            [Kind::NativeCrash, Kind::Tombstone, Kind::Tombstone]
        );
    }
}
