use android::crashes::Crash;
use async_trait::async_trait;

use super::Android;
use crate::views::crashes::Crashes;

#[async_trait(?Send)]
impl Crashes for Android {
    async fn crashes(&self) -> Result<Vec<Crash>, String> {
        android::crashes::list(&self.adb().await)
            .await
            .map_err(|error| format!("crashes failed: {error:#}"))
    }
}
