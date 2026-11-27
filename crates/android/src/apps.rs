use std::collections::{HashMap, HashSet};

use adb::Device;
use adb::libadb::Session;
use anyhow::{Result, bail, ensure};

#[derive(Clone, Debug, PartialEq)]
pub struct Package {
    pub id: String,
    pub path: String,
    /// The versionCode. 0 before Android 9, where `pm` does not print it.
    pub version: u64,
    /// System apps live outside `/data/`, where installed apps go.
    pub system: bool,
    pub launchable: bool,
    pub running: bool,
}

pub async fn list<S>(device: &Device<S>) -> Result<Vec<Package>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let listed = match device
        .sh(["pm", "list", "packages", "-f", "--show-versioncode"])
        .await
    {
        Ok(listed) => listed,
        Err(_) => device.sh(["pm", "list", "packages", "-f"]).await?,
    };
    let shown = [
        "cmd",
        "package",
        "query-activities",
        "--brief",
        "-a",
        "android.intent.action.MAIN",
        "-c",
        "android.intent.category.LAUNCHER",
    ];
    // If this query fails, the apps are still listed as not launchable.
    let launchable = device
        .sh(shown)
        .await
        .map(|said| launchable(&said))
        .unwrap_or_default();
    let running = running(device).await.unwrap_or_default();

    let mut all: Vec<Package> = packages(&listed)
        .into_iter()
        .map(|(id, (path, version))| Package {
            id: id.to_owned(),
            path: path.to_owned(),
            version,
            system: !path.starts_with("/data/"),
            launchable: launchable.contains(id),
            running: running.contains(id),
        })
        .collect();

    all.sort_by(|one, other| one.id.cmp(&other.id));

    Ok(all)
}

/// A process is named after its package, with a `:suffix` for secondary
/// processes.
pub async fn running<S>(device: &Device<S>) -> Result<HashSet<String>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let listed = device.sh(["ps", "-A", "-o", "NAME"]).await?;

    Ok(listed
        .lines()
        .map(|name| name.trim().split(':').next().unwrap_or_default().to_owned())
        .collect())
}

pub async fn sum<S>(device: &Device<S>, package: &Package) -> Result<String>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let said = device.sh(["sha256sum", &package.path]).await?;

    Ok(said
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned())
}

pub async fn open<S>(device: &Device<S>, id: &str) -> Result<()>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let started = device
        .sh([
            "monkey",
            "-p",
            id,
            "-c",
            "android.intent.category.LAUNCHER",
            "1",
        ])
        .await;
    // monkey prints that no activity was found whether or not it exits with
    // an error.
    let said = match &started {
        Ok(said) => said.clone(),
        Err(error) => format!("{error:#}"),
    };

    ensure!(
        !said.contains("No activities found"),
        "{id}: no launchable activity"
    );

    started?;

    Ok(())
}

pub async fn stop<S>(device: &Device<S>, id: &str) -> Result<()>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    device.sh(["am", "force-stop", id]).await?;

    Ok(())
}

pub async fn clear<S>(device: &Device<S>, id: &str) -> Result<()>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let said = device.sh(["pm", "clear", id]).await?;

    ensure!(said.contains("Success"), "adb failed: {}", said.trim());

    Ok(())
}

pub async fn install<S>(device: &Device<S>, apk: &[u8], name: &str) -> Result<()>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let remote = format!("/data/local/tmp/{name}.apk");

    device.push(apk, &remote).await?;

    let installed = device.sh(["pm", "install", "-r", "-d", &remote]).await;
    let _ = device.sh(["rm", "-f", &remote]).await;
    let installed = installed?;

    // Before Android 7, pm exits with success even when the install failed.
    ensure!(
        installed.contains("Success"),
        "adb failed: {}",
        installed.trim()
    );

    Ok(())
}

pub async fn uninstall<S>(device: &Device<S>, id: &str) -> Result<()>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let said = device.sh(["pm", "uninstall", id]).await?;

    ensure!(said.contains("Success"), "adb failed: {}", said.trim());

    Ok(())
}

/// Installing kills the app, and only an app that was in front is reopened
/// afterwards, because several apps started at once would fight over the
/// screen.
pub async fn in_front<S>(device: &Device<S>, id: &str) -> bool
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let activities = device
        .sh(["dumpsys", "activity", "activities"])
        .await
        .unwrap_or_default();

    resumed(&activities, id)
}

/// `dumpsys activity activities` names the resumed activity as
/// `ResumedActivity: ActivityRecord{… u0 com.example/.Main t5}`.
fn resumed(activities: &str, id: &str) -> bool {
    let activity = format!("{id}/");

    activities.lines().any(|line| {
        line.to_lowercase().contains("resumedactivity")
            && line
                .split_whitespace()
                .any(|word| word.starts_with(&activity))
    })
}

pub async fn pull<S>(device: &Device<S>, id: &str) -> Result<Vec<u8>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let said = device.sh(["pm", "path", id]).await?;
    let parts: Vec<&str> = said
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .collect();

    // shortcut: a split APK (as app stores install them) is refused. Merge
    // the parts if registering one is needed.
    let [path] = parts.as_slice() else {
        bail!(
            "{id}: split APK ({} parts), cannot be registered",
            parts.len()
        );
    };

    device.pull(path).await
}

/// `pm list packages -f --show-versioncode` prints a line such as
/// `package:/data/app/~~x==/com.example-y==/base.apk=com.example versionCode:12`
/// for each package. Without `--show-versioncode` the line ends at the id.
fn packages(list: &str) -> HashMap<&str, (&str, u64)> {
    list.lines()
        .filter_map(|line| line.trim().strip_prefix("package:")?.rsplit_once('='))
        .map(|(path, rest)| {
            let (id, version) = rest.split_once(" versionCode:").unwrap_or((rest, ""));

            (id, (path, version.trim().parse().unwrap_or_default()))
        })
        .collect()
}

/// `cmd package query-activities --brief` prints each activity on its own
/// line as `com.example/.Main`, between detail lines.
fn launchable(list: &str) -> HashSet<String> {
    list.lines()
        .map(str::trim)
        .filter(|line| !line.contains([' ', '=']))
        .filter_map(|line| Some(line.split_once('/')?.0.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_which_apps_can_be_opened() {
        let list = "2 activities found:\n  Activity #0:\n    \
            priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\n    \
            com.android.settings/.Settings\n  Activity #1:\n    \
            priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\n    \
            com.example/com.example.Main\n";

        assert_eq!(
            launchable(list),
            HashSet::from(["com.android.settings".to_owned(), "com.example".to_owned()])
        );
    }

    #[test]
    fn finds_where_packages_are() {
        let list = "package:/data/app/~~Tl3w==/com.example-Qx9A==/base.apk=com.example versionCode:213\n\
                    package:/product/app/Camera2/Camera2.apk=com.android.camera2\n";
        let found = packages(list);

        assert_eq!(
            found.get("com.example"),
            Some(&("/data/app/~~Tl3w==/com.example-Qx9A==/base.apk", 213))
        );

        assert_eq!(
            found.get("com.android.camera2"),
            Some(&("/product/app/Camera2/Camera2.apk", 0))
        );
    }

    #[test]
    fn finds_the_app_in_front() {
        let dump = "  mFocusedApp=ActivityRecord{1 u0 com.example.other/.Main t4}\n  \
            ResumedActivity: ActivityRecord{2 u0 org.com.example/.Main t5}\n";

        assert!(resumed(dump, "org.com.example"));
        assert!(!resumed(dump, "com.example"));
        assert!(!resumed(dump, "com.example.other"));
    }
}
