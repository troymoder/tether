use async_trait::async_trait;

use crate::device::unsupported;

#[async_trait(?Send)]
pub(crate) trait Location {
    /// The answer is (latitude, longitude).
    async fn whereabouts(&self) -> Result<(f64, f64), String> {
        Err(unsupported("location"))
    }

    async fn put_location(&self, _latitude: f64, _longitude: f64) -> Result<(), String> {
        Err(unsupported("location"))
    }

    async fn reset_location(&self) -> Result<(), String> {
        Err(unsupported("location"))
    }
}
