use android::apps::Package;
use async_trait::async_trait;

use crate::device::{Helper, unsupported};

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Features: u8 {
        const MANAGES = 1 << 0;
    }
}

#[async_trait(?Send)]
pub(crate) trait Apps {
    fn features(&self) -> Features {
        Features::empty()
    }

    async fn packages(&self) -> Result<Vec<Package>, String> {
        Err(unsupported("apps"))
    }

    /// The helper runs until the last handle on it goes.
    async fn labels(&self) -> Result<Helper, String> {
        Err(unsupported("app icons"))
    }

    /// The SHA-256 of the package's file.
    async fn sum(&self, _package: &Package) -> Result<String, String> {
        Err(unsupported("app sums"))
    }

    async fn open(&self, _id: &str) -> Result<(), String> {
        Err(unsupported("opening apps"))
    }

    async fn stop(&self, _id: &str) -> Result<(), String> {
        Err(unsupported("stopping apps"))
    }

    async fn install(&self, _id: &str) -> Result<(), String> {
        Err(unsupported("installing apps"))
    }

    async fn clear(&self, _id: &str) -> Result<(), String> {
        Err(unsupported("clearing app data"))
    }

    async fn uninstall(&self, _id: &str) -> Result<(), String> {
        Err(unsupported("uninstalling apps"))
    }

    async fn pull(&self, _id: &str) -> Result<Vec<u8>, String> {
        Err(unsupported("pulling apps"))
    }
}
