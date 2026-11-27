use adb::libadb::Session;
use adb::{Device, quoted};
use anyhow::{Result, ensure};

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub folder: bool,
    pub size: u64,
    /// As `ls -l` prints it, like `2026-11-20 08:13`, which sorts in time
    /// order.
    pub changed: String,
    pub name: String,
}

/// True if a package name is a plain name, because it becomes part of a
/// path and a shell argument.
pub(crate) fn is_package(name: &str) -> bool {
    name.len() <= 100
        && name.starts_with(|first: char| first.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// Wraps a script to run as a user that can reach the path: the shell user
/// in shared storage, root where the device allows it, and otherwise the app
/// whose folder the path is in, which works only for a debuggable build.
fn reaching(rooted: bool, path: &str, script: String) -> Result<String> {
    let shared = ["/sdcard", "/storage", "/data/local/tmp"]
        .iter()
        .any(|open| {
            path.strip_prefix(open)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        });

    let owner = path
        .strip_prefix("/data/data/")
        .or_else(|| path.strip_prefix("/data/user/0/"))
        .and_then(|rest| rest.split('/').next())
        .filter(|owner| is_package(owner));

    Ok(match owner {
        _ if shared => script,
        _ if rooted => format!("su 0 sh -c {}", quoted(&script)?),
        Some(owner) => format!("run-as {owner} sh -c {}", quoted(&script)?),
        None => script,
    })
}

pub(crate) fn checked(path: &str) -> Result<()> {
    ensure!(
        path.starts_with('/') && !path.contains(['\0', '\n']),
        "invalid device path"
    );

    Ok(())
}

pub(crate) fn file_name(name: &str) -> Result<&str> {
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default();

    ensure!(
        !name.is_empty() && name != "." && name != "..",
        "invalid file name"
    );

    Ok(name)
}

fn entry(line: &str) -> Option<Entry> {
    let mut rest = line;
    let mut fields = [""; 7];

    for field in &mut fields {
        rest = rest.trim_start();
        (*field, rest) = rest.split_at(rest.find(char::is_whitespace)?);
    }

    let [mode, _, _, _, size, date, time] = fields;
    let name = rest.strip_prefix(' ')?;

    let (folder, name) = match mode.chars().next()? {
        'd' => (true, name),
        // A link is listed as `name -> target`.
        'l' => (true, name.split(" -> ").next()?),
        _ => (false, name),
    };

    Some(Entry {
        folder,
        size: size.parse().ok()?,
        changed: format!("{date} {time}"),
        name: name.to_owned(),
    })
}

pub async fn list<S>(device: &Device<S>, rooted: bool, path: &str) -> Result<Vec<Entry>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    checked(path)?;

    // A trailing slash makes `ls` follow a link to a folder.
    let folder = format!("{}/", path.trim_end_matches('/'));
    let script = reaching(rooted, path, format!("ls -lA {}", quoted(&folder)?))?;
    let listed = device.script(&script).await?;
    let mut entries: Vec<Entry> = listed.lines().filter_map(entry).collect();

    entries.sort_by_key(|entry| (!entry.folder, entry.name.to_lowercase()));

    Ok(entries)
}

const FOUND: usize = 200;

/// Each entry found is its path from the folder and true for a folder.
pub async fn find<S>(
    device: &Device<S>,
    rooted: bool,
    path: &str,
    sought: &str,
) -> Result<Vec<(String, bool)>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    checked(path)?;
    ensure!(!sought.contains(['\0', '\n']), "invalid name");

    let named = quoted(&format!("*{sought}*"))?;
    let script = format!(
        "cd {} && {{ find . -mindepth 1 -type d -iname {named} | sed 's/^/d /'; \
         find . -mindepth 1 ! -type d -iname {named} | sed 's/^/f /'; }} 2>/dev/null | head -n {FOUND}",
        quoted(path)?
    );

    let found = device.script(&reaching(rooted, path, script)?).await?;

    Ok(found
        .lines()
        .filter_map(|line| {
            let (kind, at) = line.split_once(' ')?;

            Some((at.strip_prefix("./")?.to_owned(), kind == "d"))
        })
        .collect())
}

/// The kind of a path: a folder (`Some(true)`), a file (`Some(false)`), or
/// nothing there (`None`).
pub async fn kind<S>(device: &Device<S>, rooted: bool, path: &str) -> Result<Option<bool>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    checked(path)?;

    let at = quoted(path)?;
    let script = format!("if [ -d {at} ]; then echo d; elif [ -e {at} ]; then echo f; fi");
    let said = device.script(&reaching(rooted, path, script)?).await?;

    Ok(match said.trim() {
        "d" => Some(true),
        "f" => Some(false),
        _ => None,
    })
}

/// Reads a file, or a folder as a tar of everything in it.
// shortcut: the whole file is held in memory, which is fine for an app's
// data. Stream it if large videos need fetching.
pub async fn read<S>(device: &Device<S>, rooted: bool, path: &str) -> Result<Vec<u8>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    checked(path)?;

    device
        .shell(&reaching(rooted, path, read_script(path)?)?, None)
        .await
}

fn read_script(path: &str) -> Result<String> {
    let path = path.trim_end_matches('/');
    let (above, name) = path.rsplit_once('/').unwrap_or(("", path));
    let above = if above.is_empty() { "/" } else { above };

    Ok(format!(
        "if [ -d {path} ]; then tar -cf - -C {above} {name}; else cat {path}; fi",
        path = quoted(path)?,
        above = quoted(above)?,
        name = quoted(name)?,
    ))
}

pub async fn write<S>(device: &Device<S>, rooted: bool, path: &str, file: &[u8]) -> Result<()>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    checked(path)?;

    let script = reaching(rooted, path, format!("cat > {}", quoted(path)?))?;

    device.shell(&script, Some(file)).await?;

    Ok(())
}

/// Refuses paths directly under the root, so a slip cannot delete a whole tree.
fn deletable(path: &str) -> Result<()> {
    checked(path)?;
    ensure!(
        path.trim_end_matches('/').matches('/').count() >= 2,
        "{path}: refusing to delete a top-level path"
    );

    Ok(())
}

pub async fn delete<S>(device: &Device<S>, rooted: bool, path: &str) -> Result<()>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    deletable(path)?;

    let script = reaching(rooted, path, format!("rm -rf {}", quoted(path)?))?;

    device.shell(&script, None).await?;

    Ok(())
}

pub async fn receive<S>(device: &Device<S>, name: &str, file: &[u8]) -> Result<String>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let name = file_name(name)?;

    if let Some(stem) = name.strip_suffix(".apk") {
        crate::apps::install(device, file, stem).await?;

        Ok(format!("Installed {name}"))
    } else {
        device
            .push(file, &format!("/sdcard/Download/{name}"))
            .await?;

        Ok(format!("Added {name} to Download"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_folder_as_a_tar() {
        assert_eq!(
            read_script("/sdcard/Download/My files/").unwrap(),
            "if [ -d '/sdcard/Download/My files' ]; then tar -cf - -C /sdcard/Download \
             'My files'; else cat '/sdcard/Download/My files'; fi"
        );

        assert_eq!(
            read_script("/sdcard").unwrap(),
            "if [ -d /sdcard ]; then tar -cf - -C / sdcard; else cat /sdcard; fi"
        );
    }

    #[test]
    fn reads_a_listing() {
        let listed = "total 104\n\
            drwxrws--- 2 u0_a201  media_rw 4096 2026-11-20 08:13 Alarms\n\
            -rw-rw---- 1 u0_a201  media_rw  300000 2026-11-20 08:20 r and.bin\n\
            lrwxrwxrwx 1 root     root         21 2026-11-20 08:13 sdcard -> /storage/self/primary\n\
            crw-rw-rw- 1 root     root     1,   3 2026-11-20 08:13 null\n";
        let entries: Vec<Entry> = listed.lines().filter_map(entry).collect();
        let at = |folder: bool, size: u64, changed: &str, name: &str| Entry {
            folder,
            size,
            changed: changed.to_owned(),
            name: name.to_owned(),
        };

        assert_eq!(
            entries,
            [
                at(true, 4096, "2026-11-20 08:13", "Alarms"),
                at(false, 300_000, "2026-11-20 08:20", "r and.bin"),
                at(true, 21, "2026-11-20 08:13", "sdcard"),
            ]
        );
    }

    #[test]
    fn reaches_a_path_as_who_may() {
        let script = || "ls -l".to_owned();
        let reached = |rooted, path| reaching(rooted, path, script()).unwrap();

        assert_eq!(reached(true, "/sdcard/Download"), "ls -l");
        assert_eq!(reached(false, "/sdcardx"), "ls -l");
        assert_eq!(reached(true, "/data/system"), "su 0 sh -c 'ls -l'");
        assert_eq!(
            reached(false, "/data/data/com.example/files"),
            "run-as com.example sh -c 'ls -l'"
        );
        assert_eq!(reached(false, "/data/data/a;b/files"), "ls -l");
    }

    #[test]
    fn keeps_to_paths() {
        assert!(checked("relative").is_err());
        assert!(checked("/sdcard/a\nb").is_err());
        assert!(deletable("/sdcard").is_err());
        assert!(deletable("/").is_err());
        assert!(deletable("/sdcard/Download").is_ok());

        assert_eq!(file_name("C:\\photos\\a.jpg").unwrap(), "a.jpg");
        assert!(file_name("photos/..").is_err());
        assert!(file_name("").is_err());
    }
}
