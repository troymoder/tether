use android::media;
use async_trait::async_trait;

use super::Android;
use super::files::failed;
use super::helper::Helper;
use crate::views::media::Media;

#[async_trait(?Send)]
impl Media for Android {
    async fn gallery(&self) -> Result<Helper, String> {
        Helper::start(self.link, &self.adb().await).await
    }

    async fn add(&self, name: &str, file: &[u8]) -> Result<String, String> {
        media::add(&self.adb().await, name, file)
            .await
            .map_err(failed)
    }
}
