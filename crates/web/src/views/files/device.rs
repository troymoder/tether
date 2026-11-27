pub(crate) use android::files::Entry;
use async_trait::async_trait;

use crate::device::unsupported;

#[async_trait(?Send)]
pub(crate) trait Files {
    /// The folders to start from, as (path, label).
    async fn places(&self) -> Result<Vec<(String, String)>, String> {
        Err(unsupported("places"))
    }

    /// The entries of a folder, the folders first.
    async fn list(&self, _path: &str) -> Result<Vec<Entry>, String> {
        Err(unsupported("files"))
    }

    /// The entries under a folder, all the way down, with `sought` in their
    /// name: each by its path from the folder, and true for a folder.
    async fn find(&self, _path: &str, _sought: &str) -> Result<Vec<(String, bool)>, String> {
        Err(unsupported("search"))
    }

    /// True for a folder, false for a file, None for nothing there.
    async fn entry_kind(&self, _path: &str) -> Result<Option<bool>, String> {
        Err(unsupported("files"))
    }

    /// The bytes of a file, or a folder as a tar of everything in it.
    async fn read(&self, _path: &str) -> Result<Vec<u8>, String> {
        Err(unsupported("files"))
    }

    async fn write(&self, _path: &str, _file: &[u8]) -> Result<(), String> {
        Err(unsupported("files"))
    }

    /// Deletes a file, or a folder with everything in it.
    async fn delete(&self, _path: &str) -> Result<(), String> {
        Err(unsupported("files"))
    }
}
