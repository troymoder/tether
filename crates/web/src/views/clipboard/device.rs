use async_trait::async_trait;

use crate::device::unsupported;
use crate::views::screen::Control;

/// A device that reads and sets its clipboard through its screen does so
/// through `screen`.
#[async_trait(?Send)]
pub(crate) trait Clipboard {
    /// Asks for the text on it, which comes to `Board::told`.
    async fn read_clipboard(&self, _screen: Control) -> Result<(), String> {
        Err(unsupported("clipboard"))
    }

    async fn write_clipboard(&self, _screen: Control, _text: String) -> Result<(), String> {
        Err(unsupported("clipboard"))
    }
}
