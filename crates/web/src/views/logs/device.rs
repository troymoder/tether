use std::pin::Pin;

pub(crate) use android::logcat::Process;
use async_trait::async_trait;
use futures_util::Stream;

use crate::device::unsupported;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Sources: u8 {
        const DEVICE = 1 << 0;
        /// The output of the emulator it runs in, which tetherd keeps.
        const OUTPUT = 1 << 1;
    }
}

/// The lines the device still has come first, then each new batch as it
/// comes. Dropping it stops following.
pub(crate) type Lines = Pin<Box<dyn Stream<Item = Result<Vec<String>, String>>>>;

#[async_trait(?Send)]
pub(crate) trait Logs {
    fn sources(&self) -> Sources {
        Sources::empty()
    }

    /// Follows its log, starting with up to `most` of the lines it has.
    async fn follow(&self, _most: usize) -> Result<Lines, String> {
        Err(unsupported("log"))
    }

    /// Its processes, since a line names its process only by pid.
    async fn processes(&self) -> Result<Vec<Process>, String> {
        Err(unsupported("processes"))
    }

    /// The package of the app in front. None on its home screen.
    async fn front(&self) -> Result<Option<String>, String> {
        Err(unsupported("front app"))
    }

    /// Its emulator's output so far, a line per time it printed: milliseconds
    /// since the epoch, a tab, and what it printed.
    async fn output(&self) -> Result<String, String> {
        Err(unsupported("emulator output"))
    }
}
