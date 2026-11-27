use android::tweaks::{Tweak, Tweaks as Setting};
use async_trait::async_trait;

use super::Android;
use crate::views::preferences::Tweaks;

#[async_trait(?Send)]
impl Tweaks for Android {
    async fn tweaks(&self) -> Result<Setting, String> {
        android::tweaks::read(&self.adb().await)
            .await
            .map_err(|error| format!("{error:#}"))
    }

    async fn tweak(&self, tweak: &Tweak) -> Result<String, String> {
        android::tweaks::set(&self.adb().await, tweak)
            .await
            .map_err(|error| format!("{error:#}"))
    }
}
