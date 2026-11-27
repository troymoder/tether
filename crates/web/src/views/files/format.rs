use crate::app::icons;

pub(super) fn sign(name: &str) -> &'static str {
    let ends = name
        .rsplit_once('.')
        .map_or(String::new(), |(_, ends)| ends.to_lowercase());

    match ends.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "bmp" | "svg" => icons::IMAGE,
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "3gp" => icons::CAMERA,
        "apk" | "aab" | "ipa" | "zip" | "tar" | "gz" | "tgz" | "7z" | "rar" => icons::APPS,
        "mp3" | "wav" | "ogg" | "flac" | "m4a" => icons::AUDIO,
        "db" | "sqlite" | "sqlite3" => icons::DATABASE,
        _ => icons::FILE,
    }
}

pub(super) fn above(path: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/".to_owned(),
        Some((above, _)) => above.to_owned(),
    }
}

pub(super) fn within(folder: &str, name: &str) -> String {
    format!("{}/{name}", folder.trim_end_matches('/'))
}

/// A folder arrives as a tar of everything in it.
pub(super) fn saved_as(path: &str, folder: bool) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);

    if folder {
        format!("{name}.tar")
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_put_in_its_folder() {
        assert_eq!(within("/sdcard", "a.txt"), "/sdcard/a.txt");
        assert_eq!(within("/sdcard/", "DCIM"), "/sdcard/DCIM");
        assert_eq!(within("/", "sdcard"), "/sdcard");
    }

    #[test]
    fn a_folder_is_saved_as_a_tar() {
        assert_eq!(saved_as("/sdcard/Download/a.txt", false), "a.txt");
        assert_eq!(saved_as("/sdcard/DCIM", true), "DCIM.tar");
    }

    #[test]
    fn above_is_the_parent_folder() {
        assert_eq!(above("/sdcard/Download/a.txt"), "/sdcard/Download");
        assert_eq!(above("/sdcard/"), "/");
        assert_eq!(above("/"), "/");
        assert_eq!(above("name"), "/");
    }
}
