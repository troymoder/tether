//! An Android device's clipboard, which scrcpy reads and sets over the
//! screen's session, so it only works while the screen is shown.

use async_trait::async_trait;

use super::Android;
use crate::views::clipboard::Clipboard;
use crate::views::screen::{Control, Input};

#[async_trait(?Send)]
impl Clipboard for Android {
    async fn read_clipboard(&self, screen: Control) -> Result<(), String> {
        screen.send(Input::ReadClipboard);

        Ok(())
    }

    async fn write_clipboard(&self, screen: Control, text: String) -> Result<(), String> {
        screen.send(Input::Clipboard(text));

        Ok(())
    }
}
