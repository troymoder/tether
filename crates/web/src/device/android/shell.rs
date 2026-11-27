use async_trait::async_trait;
use futures_channel::mpsc;
use futures_util::{StreamExt, stream};
use leptos::task::spawn_local;

use super::Android;
use crate::views::shell::{Ask, Output, Shell};

#[async_trait(?Send)]
impl Shell for Android {
    async fn shell(
        &self,
        size: (u16, u16),
        mut asks: mpsc::UnboundedReceiver<Ask>,
    ) -> Result<Output, String> {
        let device = self.adb().await;
        let adb::Process { mut input, output } = android::shell::open(&device, size)
            .await
            .map_err(|error| format!("shell failed: {error:#}"))?;

        // A single task writes, so keystrokes and resizes keep their order.
        spawn_local(async move {
            while let Some(ask) = asks.next().await {
                let written = match ask {
                    Ask::Typed(bytes) => input.write(&bytes).await,
                    Ask::Size { columns, rows } => input.resize(rows, columns).await,
                };

                if written.is_err() {
                    break;
                }
            }

            let _ = input.shut().await;
        });

        Ok(Box::pin(stream::unfold(output, |mut output| async move {
            match output.next().await {
                Ok(Some(said)) => Some((Ok(said), output)),
                Ok(None) => None,
                Err(error) => Some((Err(format!("shell failed: {error:#}")), output)),
            }
        })))
    }
}
