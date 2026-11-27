use adb::libadb::Session;
use adb::{Device, quoted};
use anyhow::Result;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::files;

pub async fn add<S>(device: &Device<S>, name: &str, file: &[u8]) -> Result<String>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let name = files::file_name(name)?;
    let extension = name.rsplit('.').next().unwrap_or_default().to_lowercase();
    let folder = match extension.as_str() {
        "mp4" | "mkv" | "webm" | "mov" | "3gp" | "avi" => "Movies",
        _ => "Pictures",
    };

    let path = format!("/sdcard/{folder}/{name}");

    files::write(device, false, &path, file).await?;

    // From Android 11 the media scanner picks the file up by itself. Older
    // releases need the broadcast, and a second scan does no harm.
    let address = format!("file://{}", utf8_percent_encode(&path, IN_PATH));
    let scan = format!(
        "am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d {}",
        quoted(&address)?
    );

    let _ = device.shell(&scan, None).await;

    Ok(format!("Added {name} to {folder}"))
}

/// Everything but letters, digits, `/` and `-._~` is percent-encoded, so a
/// name with `#`, `%` or a space still resolves.
const IN_PATH: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'/')
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
