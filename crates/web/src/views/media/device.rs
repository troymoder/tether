use async_trait::async_trait;

use crate::device::{Helper, unsupported};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) changed: String,
}

impl Item {
    pub(crate) fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

#[async_trait(?Send)]
pub(crate) trait Media {
    /// The helper runs until the last handle on it is dropped.
    async fn gallery(&self) -> Result<Helper, String> {
        Err(unsupported("media"))
    }

    /// Returns where on the device the file was put.
    async fn add(&self, _name: &str, _file: &[u8]) -> Result<String, String> {
        Err(unsupported("media"))
    }
}
