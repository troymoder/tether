use android::crashes::Crash;
use async_trait::async_trait;

use crate::device::unsupported;

#[async_trait(?Send)]
pub(crate) trait Crashes {
    /// The latest first.
    async fn crashes(&self) -> Result<Vec<Crash>, String> {
        Err(unsupported("crashes"))
    }
}
