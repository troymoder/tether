use async_trait::async_trait;
use protocol::Request;

use super::Android;
use crate::views::location::Location;

#[async_trait(?Send)]
impl Location for Android {
    async fn whereabouts(&self) -> Result<(f64, f64), String> {
        android::location::whereabouts(&self.adb().await)
            .await
            .map_err(|error| format!("{error:#}"))
    }

    /// An emulator running Android before 12 is set through its console,
    /// which only tetherd reaches.
    async fn put_location(&self, latitude: f64, longitude: f64) -> Result<(), String> {
        let device = self.adb().await;
        let told = android::location::put(&device, latitude, longitude).await;
        match told {
            Ok(()) => Ok(()),
            Err(_) if self.id.is_emulator() => self
                .link
                .call(Request::GeoFix {
                    device: self.id.clone(),
                    latitude,
                    longitude,
                })
                .await
                .map(drop),
            Err(error) => Err(format!("{error:#}")),
        }
    }

    async fn reset_location(&self) -> Result<(), String> {
        android::location::reset(&self.adb().await)
            .await
            .map_err(|error| format!("{error:#}"))
    }
}
