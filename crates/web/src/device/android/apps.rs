use android::apps::{self, Package};
use async_trait::async_trait;
use protocol::Request;

use super::Android;
use super::helper::Helper;
use crate::views::apps::{Apps, Features};

#[async_trait(?Send)]
impl Apps for Android {
    fn features(&self) -> Features {
        Features::all()
    }

    async fn packages(&self) -> Result<Vec<Package>, String> {
        apps::list(&self.adb().await)
            .await
            .map_err(|error| format!("apps failed: {error:#}"))
    }

    async fn labels(&self) -> Result<Helper, String> {
        Helper::start(self.link, &self.adb().await).await
    }

    async fn sum(&self, package: &Package) -> Result<String, String> {
        apps::sum(&self.adb().await, package)
            .await
            .map_err(|error| format!("sum failed: {error:#}"))
    }

    async fn open(&self, id: &str) -> Result<(), String> {
        apps::open(&self.adb().await, id)
            .await
            .map_err(|error| format!("open failed: {error:#}"))
    }

    async fn stop(&self, id: &str) -> Result<(), String> {
        apps::stop(&self.adb().await, id)
            .await
            .map_err(|error| format!("stop failed: {error:#}"))
    }

    /// Installing stops the app, so it is opened again if it was in front
    /// before.
    async fn install(&self, id: &str) -> Result<(), String> {
        let shown = apps::in_front(&self.adb().await, id).await;

        // tetherd cannot reach a phone on this browser's WebUSB, so the page
        // installs on it itself.
        if self.link.held(&self.id.serial()).is_some() {
            self.link.install_here(&self.id, id).await?;
        } else {
            self.link
                .call(Request::InstallApp {
                    device: self.id.clone(),
                    id: id.to_owned(),
                })
                .await?;
        }

        if shown {
            self.open(id).await?;
        }

        Ok(())
    }

    async fn clear(&self, id: &str) -> Result<(), String> {
        apps::clear(&self.adb().await, id)
            .await
            .map_err(|error| format!("clear failed: {error:#}"))
    }

    async fn uninstall(&self, id: &str) -> Result<(), String> {
        apps::uninstall(&self.adb().await, id)
            .await
            .map_err(|error| format!("uninstall failed: {error:#}"))
    }

    async fn pull(&self, id: &str) -> Result<Vec<u8>, String> {
        apps::pull(&self.adb().await, id)
            .await
            .map_err(|error| format!("pull failed: {error:#}"))
    }
}
