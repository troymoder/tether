//! A content-addressed cache in the browser's origin private file system. A
//! [`Key`]'s entry is the file `cache/<kind>/<SHA-256 of the key as JSON>`,
//! so a key should change whenever what it stands for does. Every failure
//! reads as a miss, since a cache can always be filled again.
//!
//! The IndexedDB table `cache` tracks each entry's size and last use. Once
//! the entries pass [`limit`], the least recently used go.

use std::sync::atomic::{AtomicBool, Ordering};

use js_sys::{ArrayBuffer, Uint8Array};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemRemoveOptions, FileSystemWritableFileStream,
};

use crate::app::storage::{self, CACHE, CACHE_COUNTS};

const HITS: &str = "hits";
const MISSES: &str = "misses";

const FALLBACK: u64 = 256 * 1024 * 1024;

/// In bytes: a quarter of the quota the browser gives this origin. The quota
/// covers IndexedDB and everything else the page keeps too, and going over it
/// can have the browser clear the whole origin.
pub(crate) async fn limit() -> u64 {
    let quota = async {
        let estimate = JsFuture::from(web_sys::window()?.navigator().storage().estimate().ok()?)
            .await
            .ok()?;

        js_sys::Reflect::get(&estimate, &"quota".into())
            .ok()?
            .as_f64()
    };

    match quota.await {
        Some(quota) if quota > 0.0 => (quota / 4.0) as u64,
        _ => FALLBACK,
    }
}

#[derive(Clone)]
pub(crate) struct Stats {
    pub(crate) entries: usize,
    /// In bytes.
    pub(crate) size: u64,
    pub(crate) limit: u64,
    pub(crate) hits: f64,
    pub(crate) misses: f64,
}

impl Stats {
    pub(crate) fn hit_rate(&self) -> Option<f64> {
        let reads = self.hits + self.misses;

        (reads > 0.0).then(|| self.hits / reads)
    }
}

pub(crate) async fn stats() -> Stats {
    let entries: Vec<Entry> = storage::all(CACHE).await;

    Stats {
        entries: entries.len(),
        size: entries.iter().map(|entry| entry.size).sum(),
        limit: limit().await,
        hits: storage::counted(CACHE_COUNTS, HITS).await,
        misses: storage::counted(CACHE_COUNTS, MISSES).await,
    }
}

pub(crate) async fn clear() {
    let root = async {
        JsFuture::from(web_sys::window()?.navigator().storage().get_directory())
            .await
            .ok()?
            .dyn_into::<FileSystemDirectoryHandle>()
            .ok()
    };

    if let Some(root) = root.await {
        let options = FileSystemRemoveOptions::new();
        options.set_recursive(true);

        let _ = JsFuture::from(root.remove_entry_with_options("cache", &options)).await;
    }

    let _ = storage::clear(CACHE).await;
    let _ = storage::clear(CACHE_COUNTS).await;
}

/// An entry's name comes from the key as JSON, because Rust's own `Hash` may
/// change with the compiler.
pub(crate) trait Key: Serialize {
    /// The folder its entries are kept in, which sets one kind of key apart
    /// from another with the same fields. Changing it loses that kind's
    /// entries until they are swept.
    const KIND: &'static str;
}

#[derive(Serialize, Deserialize)]
struct Entry {
    kind: String,
    name: String,
    size: u64,
    /// Milliseconds since 1970.
    used: f64,
}

pub(crate) async fn read<K: Key>(key: &K) -> Option<Vec<u8>> {
    let found = lookup(key).await;
    let counted = if found.is_some() { HITS } else { MISSES };

    leptos::task::spawn_local(async move {
        let _ = storage::count(CACHE_COUNTS, counted).await;
    });

    found
}

async fn lookup<K: Key>(key: &K) -> Option<Vec<u8>> {
    let name = named(key)?;
    let file = entry(K::KIND, &name, false).await?;
    let blob: Blob = JsFuture::from(file.get_file())
        .await
        .ok()?
        .dyn_into()
        .ok()?;
    let buffer: ArrayBuffer = JsFuture::from(blob.array_buffer())
        .await
        .ok()?
        .dyn_into()
        .ok()?;
    let data = Uint8Array::new(&buffer).to_vec();

    leptos::task::spawn_local(used(K::KIND, name, data.len() as u64));

    Some(data)
}

pub(crate) async fn write<K: Key>(key: &K, data: &[u8]) {
    let Some(name) = named(key) else {
        return;
    };

    // The row comes first so a file is never left without one. A row whose
    // file never got written only costs its sweep a removal that fails.
    used(K::KIND, name.clone(), data.len() as u64).await;

    let Some(file) = entry(K::KIND, &name, true).await else {
        return;
    };
    let Ok(stream) = JsFuture::from(file.create_writable()).await else {
        return;
    };
    let stream: FileSystemWritableFileStream = stream.unchecked_into();

    if let Ok(written) = stream.write_with_u8_array(data) {
        let _ = JsFuture::from(written).await;
    }

    let _ = JsFuture::from(stream.close()).await;

    leptos::task::spawn_local(sweep());
}

async fn used(kind: &str, name: String, size: u64) {
    let row = Entry {
        kind: kind.to_owned(),
        name,
        size,
        used: js_sys::Date::now(),
    };

    let _ = storage::put(CACHE, format!("{}/{}", row.kind, row.name), row).await;
}

async fn sweep() {
    static SWEEPING: AtomicBool = AtomicBool::new(false);

    if SWEEPING.swap(true, Ordering::AcqRel) {
        return;
    }

    let mut entries: Vec<Entry> = storage::all(CACHE).await;
    let mut total: u64 = entries.iter().map(|entry| entry.size).sum();
    let limit = limit().await;
    // Below the limit, so a full cache is not swept again on every write.
    let swept = limit / 10 * 9;

    if total > limit {
        entries.sort_by(|one, other| one.used.total_cmp(&other.used));

        for gone in entries {
            if total <= swept {
                break;
            }

            if let Some(folder) = folder(&gone.kind, false).await {
                let _ = JsFuture::from(folder.remove_entry(&gone.name)).await;
            }

            let _ = storage::remove(CACHE, format!("{}/{}", gone.kind, gone.name)).await;
            total = total.saturating_sub(gone.size);
        }
    }

    SWEEPING.store(false, Ordering::Release);
}

async fn folder(kind: &str, create: bool) -> Option<FileSystemDirectoryHandle> {
    let mut at: FileSystemDirectoryHandle =
        JsFuture::from(web_sys::window()?.navigator().storage().get_directory())
            .await
            .ok()?
            .dyn_into()
            .ok()?;
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(create);

    for part in ["cache", kind] {
        at = JsFuture::from(at.get_directory_handle_with_options(part, &options))
            .await
            .ok()?
            .dyn_into()
            .ok()?;
    }

    Some(at)
}

async fn entry(kind: &str, name: &str, create: bool) -> Option<FileSystemFileHandle> {
    let folder = folder(kind, create).await?;
    let options = FileSystemGetFileOptions::new();
    options.set_create(create);

    JsFuture::from(folder.get_file_handle_with_options(name, &options))
        .await
        .ok()?
        .dyn_into()
        .ok()
}

fn named<K: Key>(key: &K) -> Option<String> {
    let json = serde_json::to_vec(key).ok()?;

    Some(format!("{:x}", Sha256::digest(json)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Thumbnail<'a> {
        path: &'a str,
        size: u64,
        modified: u64,
    }

    impl Key for Thumbnail<'_> {
        const KIND: &'static str = "thumbnail";
    }

    #[test]
    fn the_hit_rate_is_the_share_of_reads_that_hit() {
        let stats = |hits, misses| Stats {
            entries: 0,
            size: 0,
            limit: 0,
            hits,
            misses,
        };

        assert_eq!(stats(0.0, 0.0).hit_rate(), None);
        assert_eq!(stats(3.0, 1.0).hit_rate(), Some(0.75));
    }

    #[test]
    fn a_key_names_the_same_entry_until_it_changes() {
        let one = Thumbnail {
            path: "/sdcard/DCIM/a.jpg",
            size: 10,
            modified: 1,
        };
        let again = Thumbnail {
            path: "/sdcard/DCIM/a.jpg",
            size: 10,
            modified: 1,
        };
        let changed = Thumbnail {
            modified: 2,
            ..again
        };

        assert_eq!(named(&one), named(&again));
        assert_ne!(named(&one), named(&changed));
        // Pinned, so a change to how names are made shows here and not as
        // every entry quietly missing.
        assert_eq!(
            named(&one).as_deref(),
            Some("5ecf01d5d236937edbda350c5f3d81df0f43d7e96de29028b70218bb2279ccc5")
        );
    }
}
