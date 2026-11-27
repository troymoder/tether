//! An app's own folder is only reachable on a device that grants root, such
//! as an emulator, or when the app is debuggable.

use android::files;
use async_trait::async_trait;
use leptos::prelude::*;

use super::Android;
use crate::link::ServerStoreFields;
use crate::views::files::{Entry, Files};

pub(super) fn failed(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}

#[async_trait(?Send)]
impl Files for Android {
    async fn places(&self) -> Result<Vec<(String, String)>, String> {
        let mut places: Vec<(String, String)> = [
            ("/sdcard", "Shared storage"),
            ("/sdcard/Download", "Download"),
            ("/sdcard/Pictures", "Pictures"),
            ("/sdcard/DCIM", "Camera"),
            ("/data/local/tmp", "Temporary files"),
        ]
        .map(|(path, label)| (path.to_owned(), label.to_owned()))
        .into();

        // The storage folders are still offered when listing apps fails.
        let Ok(packages) = android::apps::list(&self.adb().await).await else {
            return Ok(places);
        };

        let mut apps: Vec<String> = packages
            .into_iter()
            .filter(|package| !package.system)
            .map(|package| package.id)
            .collect();

        self.link.server.apps().with_untracked(|registry| {
            let registered = |id: &str| registry.iter().find(|app| app.id == id);

            apps.sort_by_key(|id| registered(id).is_none());

            for id in apps {
                let name = registered(&id)
                    .and_then(|app| app.name.clone())
                    .unwrap_or_else(|| id.clone());

                places.push((
                    format!("/data/data/{id}"),
                    format!("{name} · its own files"),
                ));
            }
        });

        Ok(places)
    }

    async fn list(&self, path: &str) -> Result<Vec<Entry>, String> {
        let device = self.adb().await;

        files::list(&device, self.rooted().await, path)
            .await
            .map_err(failed)
    }

    async fn find(&self, path: &str, sought: &str) -> Result<Vec<(String, bool)>, String> {
        let device = self.adb().await;

        files::find(&device, self.rooted().await, path, sought)
            .await
            .map_err(failed)
    }

    async fn entry_kind(&self, path: &str) -> Result<Option<bool>, String> {
        let device = self.adb().await;

        files::kind(&device, self.rooted().await, path)
            .await
            .map_err(failed)
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let device = self.adb().await;

        files::read(&device, self.rooted().await, path)
            .await
            .map_err(failed)
    }

    async fn write(&self, path: &str, file: &[u8]) -> Result<(), String> {
        let device = self.adb().await;

        files::write(&device, self.rooted().await, path, file)
            .await
            .map_err(failed)
    }

    async fn delete(&self, path: &str) -> Result<(), String> {
        let device = self.adb().await;

        files::delete(&device, self.rooted().await, path)
            .await
            .map_err(failed)
    }
}
