use std::time::Duration;

use futures_channel::oneshot;
use leptos::html;
use leptos::prelude::*;

pub(crate) fn size(bytes: u64) -> String {
    let mut size = bytes as f64;
    let mut unit = "B";

    for larger in ["KB", "MB", "GB"] {
        if size < 1000.0 {
            break;
        }

        size /= 1000.0;
        unit = larger;
    }

    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {unit}")
    }
}

pub(crate) fn counted(count: usize) -> String {
    match count {
        1 => "1 item".to_owned(),
        count => format!("{count} items"),
    }
}

/// The local time of day, in the form the device's log uses.
pub(crate) fn clock(milliseconds: f64) -> String {
    let date = js_sys::Date::new(&milliseconds.into());

    format!(
        "{:02}:{:02}:{:02}.{:03}",
        date.get_hours(),
        date.get_minutes(),
        date.get_seconds(),
        date.get_milliseconds()
    )
}

pub(crate) async fn sleep(time: Duration) {
    let (done, waited) = oneshot::channel();

    set_timeout(
        move || {
            let _ = done.send(());
        },
        time,
    );

    let _ = waited.await;
}

/// Empties the input afterwards, so that the same file can be chosen again.
pub(crate) fn chosen(chooser: NodeRef<html::Input>, each: impl Fn(web_sys::File)) {
    let Some(chooser) = chooser.get_untracked() else {
        return;
    };

    if let Some(files) = chooser.files() {
        for index in 0..files.length() {
            if let Some(file) = files.get(index) {
                each(file);
            }
        }
    }

    chooser.set_value("");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_in_decimal_units() {
        assert_eq!(size(999), "999 B");
        assert_eq!(size(1000), "1.0 KB");
        assert_eq!(size(1_500_000), "1.5 MB");
        assert_eq!(size(2_000_000_000_000), "2000.0 GB");
    }

    #[test]
    fn counts_in_words() {
        assert_eq!(counted(1), "1 item");
        assert_eq!(counted(3), "3 items");
    }
}
