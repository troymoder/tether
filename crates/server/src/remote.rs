//! A trusted client sees the whole file system of this machine, as its one user
//! does.

use std::collections::VecDeque;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use protocol::{Item, ItemKind, Looked, Match};

const VIDEOS: [&str; 7] = ["mp4", "m4v", "mov", "mkv", "webm", "avi", "mpg"];
const PICTURES: [&str; 6] = ["jpg", "jpeg", "png", "webp", "gif", "bmp"];

const FOUND: usize = 200;
const LOOKED_IN: usize = 20_000;

/// True for a picture, false for a video, None for anything else.
fn kind(name: &str) -> Option<bool> {
    let end = name.rsplit_once('.')?.1.to_lowercase();

    if VIDEOS.contains(&end.as_str()) {
        Some(false)
    } else if PICTURES.contains(&end.as_str()) {
        Some(true)
    } else {
        None
    }
}

pub(crate) fn picture(path: &Path) -> Option<bool> {
    kind(&path.file_name()?.to_string_lossy())
}

pub(crate) fn whole(path: &str) -> Result<PathBuf> {
    ensure!(!path.contains('\0'), "invalid path");

    let path = if let Some(rest) = path.strip_prefix('~') {
        let home = std::env::var_os("HOME").context("no home folder")?;

        PathBuf::from(home).join(rest.trim_start_matches('/'))
    } else {
        PathBuf::from(path)
    };

    ensure!(path.is_absolute(), "not a whole path: {}", path.display());

    Ok(path)
}

/// Blocks, as it reads the disk.
pub(crate) fn look(path: &str) -> Result<Looked> {
    let path = whole(path)?;
    let meta = std::fs::metadata(&path)
        .with_context(|| format!("No such folder or file: {}", path.display()))?;

    if !meta.is_dir() {
        return Ok(Looked::File {
            path: path.display().to_string(),
        });
    }

    let mut folders = Vec::new();
    let mut files = Vec::new();

    for entry in std::fs::read_dir(&path)
        .with_context(|| path.display().to_string())?
        .flatten()
    {
        match listed(&entry) {
            Some(item) if matches!(item.kind, ItemKind::Folder { .. }) => folders.push(item),
            Some(item) => files.push(item),
            None => {}
        }
    }

    folders.sort_by_key(|item| item.name.to_lowercase());
    files.sort_by_key(|item| item.name.to_lowercase());
    folders.extend(files);

    Ok(Looked::Folder {
        path: path.display().to_string(),
        items: folders,
    })
}

/// Hidden entries are left out, as a file manager does.
fn listed(entry: &DirEntry) -> Option<Item> {
    let name = entry.file_name().to_string_lossy().into_owned();

    if name.starts_with('.') {
        return None;
    }

    let meta = std::fs::metadata(entry.path()).ok()?;

    if meta.is_dir() {
        let items = std::fs::read_dir(entry.path()).map_or(0, |inside| inside.count());

        Some(Item {
            name,
            kind: ItemKind::Folder {
                items: u32::try_from(items).unwrap_or(u32::MAX),
            },
        })
    } else if kind(&name).is_some() {
        Some(Item {
            name,
            kind: ItemKind::File { size: meta.len() },
        })
    } else {
        None
    }
}

/// Blocks, as it reads the disk.
pub(crate) fn find(path: &str, sought: &str) -> Result<Vec<Match>> {
    let top = whole(path)?;
    let sought = sought.to_lowercase();
    let mut found = Vec::new();
    let mut waiting = VecDeque::from([top.clone()]);
    let mut looked = 0;

    while let Some(folder) = waiting.pop_front() {
        looked += 1;

        if looked > LOOKED_IN || found.len() >= FOUND {
            break;
        }

        let Ok(inside) = std::fs::read_dir(&folder) else {
            continue;
        };

        for entry in inside.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();

            if name.starts_with('.') {
                continue;
            }

            // The entry's own type, which does not follow links, so links that
            // lead back up are not walked into.
            let Ok(kind_of) = entry.file_type() else {
                continue;
            };

            let folder = kind_of.is_dir();
            let shown = folder || kind(&name).is_some();

            if shown && name.to_lowercase().contains(&sought) && found.len() < FOUND {
                let relative = entry.path();
                let relative = relative.strip_prefix(&top).unwrap_or(&relative);

                found.push(Match {
                    path: relative.display().to_string(),
                    folder,
                });
            }

            if folder {
                waiting.push_back(entry.path());
            }
        }
    }

    Ok(found)
}

pub(crate) async fn size(ffmpeg: &Path, file: &Path) -> Result<(u16, u16)> {
    // ffprobe is beside ffmpeg, or on the PATH when ffmpeg is.
    let ffprobe = match ffmpeg
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        Some(folder) => folder.join("ffprobe"),
        None => PathBuf::from("ffprobe"),
    };
    let said = tokio::process::Command::new(&ffprobe)
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=width,height", "-of", "csv=p=0"])
        .arg(file)
        .output()
        .await
        .with_context(|| ffprobe.display().to_string())?;

    fitted(&String::from_utf8_lossy(&said.stdout))
        .with_context(|| format!("not a video or a picture: {}", file.display()))
}

/// ffprobe prints the size as `width,height`.
fn fitted(said: &str) -> Option<(u16, u16)> {
    let (width, height) = said
        .trim()
        .split_once(',')
        .and_then(|(width, height)| Some((width.parse::<f64>().ok()?, height.parse::<f64>().ok()?)))
        .filter(|(width, height)| *width > 0.0 && *height > 0.0)?;
    let scale = (1920.0 / width.max(height)).min(1.0);
    let even = |side: f64| ((side * scale) as u16 / 2 * 2).max(2);

    Some((even(width), even(height)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_then_what_a_camera_can_show() {
        let top = std::env::temp_dir().join(format!("tetherd-remote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);

        std::fs::create_dir_all(top.join("scenes/night")).unwrap();
        std::fs::write(top.join("wide.mp4"), b"v").unwrap();
        std::fs::write(top.join("notes.txt"), b"t").unwrap();
        std::fs::write(top.join("scenes/night/Moon.PNG"), b"p").unwrap();
        std::fs::write(top.join(".hidden.mp4"), b"h").unwrap();

        let Looked::Folder { items, .. } = look(&top.display().to_string()).unwrap() else {
            panic!("a folder");
        };

        let names: Vec<_> = items
            .iter()
            .map(|item| (item.name.as_str(), item.kind))
            .collect();

        assert_eq!(
            names,
            [
                ("scenes", ItemKind::Folder { items: 1 }),
                ("wide.mp4", ItemKind::File { size: 1 })
            ]
        );
        assert!(matches!(
            look(&top.join("wide.mp4").display().to_string()).unwrap(),
            Looked::File { .. }
        ));
        assert!(look(&top.join("gone").display().to_string()).is_err());
        assert!(look("relative/path").is_err());

        let found = find(&top.display().to_string(), "moon").unwrap();

        assert_eq!(
            found,
            [Match {
                path: "scenes/night/Moon.PNG".to_owned(),
                folder: false,
            }]
        );

        let _ = std::fs::remove_dir_all(&top);
    }

    #[test]
    fn fits_a_size_within_1920_and_makes_it_even() {
        assert_eq!(fitted("1280,720\n"), Some((1280, 720)));
        assert_eq!(fitted("3840,2160"), Some((1920, 1080)));
        assert_eq!(fitted("641,481"), Some((640, 480)));
        assert_eq!(fitted("0,480"), None);
        assert_eq!(fitted(""), None);
    }
}
