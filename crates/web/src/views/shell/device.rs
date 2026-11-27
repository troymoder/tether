use std::pin::Pin;

use adb::Said;
use async_trait::async_trait;
use futures_channel::mpsc;
use futures_util::stream::Stream;

use crate::device::unsupported;

pub(crate) enum Ask {
    Typed(Vec<u8>),
    Size { columns: u16, rows: u16 },
}

/// Dropping it leaves the shell running until its `asks` channel closes.
pub(crate) type Output = Pin<Box<dyn Stream<Item = Result<Said, String>>>>;

#[async_trait(?Send)]
pub(crate) trait Shell {
    /// `size` is (columns, rows). The shell reads `asks` until that channel
    /// closes, and then ends.
    async fn shell(
        &self,
        _size: (u16, u16),
        _asks: mpsc::UnboundedReceiver<Ask>,
    ) -> Result<Output, String> {
        Err(unsupported("shell"))
    }
}
