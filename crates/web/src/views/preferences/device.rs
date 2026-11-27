use android::tweaks::{Tweak, Tweaks as Setting};
use async_trait::async_trait;

use crate::device::unsupported;

#[async_trait(?Send)]
pub(crate) trait Tweaks {
    async fn tweaks(&self) -> Result<Setting, String> {
        Err(unsupported("tweaks"))
    }

    /// Returns a line that describes the change.
    async fn tweak(&self, _tweak: &Tweak) -> Result<String, String> {
        Err(unsupported("tweaks"))
    }
}
