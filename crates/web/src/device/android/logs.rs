use adb::Input;
use android::logcat::{self, Process};
use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use leptos::task::spawn_local;

use super::Android;
use crate::device::emulator_output;
use crate::link::usb::Phone;
use crate::views::logs::{Lines, Logs, Sources};

/// Ends logcat when dropped. Dropping its input alone would leave the
/// pending read waiting.
struct Stop(Option<Input<Phone>>);

impl Drop for Stop {
    fn drop(&mut self) {
        if let Some(input) = self.0.take() {
            spawn_local(async move {
                let _ = input.shut().await;
            });
        }
    }
}

fn failed(error: impl std::fmt::Display) -> String {
    format!("log failed: {error:#}")
}

#[async_trait(?Send)]
impl Logs for Android {
    fn sources(&self) -> Sources {
        if self.id.is_emulator() {
            Sources::DEVICE | Sources::OUTPUT
        } else {
            Sources::DEVICE
        }
    }

    async fn follow(&self, most: usize) -> Result<Lines, String> {
        let device = self.adb().await;
        let logcat::Following { kept, stop, lines } =
            logcat::follow(&device, most).await.map_err(failed)?;
        let new = stream::unfold((lines, Stop(Some(stop))), |(mut lines, stop)| async move {
            match lines.next().await {
                Ok(Some(new)) => Some((Ok(new), (lines, stop))),
                Ok(None) => None,
                Err(error) => Some((Err(failed(error)), (lines, stop))),
            }
        });

        Ok(Box::pin(stream::iter([Ok(kept)]).chain(new)))
    }

    async fn processes(&self) -> Result<Vec<Process>, String> {
        logcat::processes(&self.adb().await).await.map_err(failed)
    }

    async fn front(&self) -> Result<Option<String>, String> {
        let front = logcat::front(&self.adb().await).await.map_err(failed)?;

        // logcat gives an empty package name on the home screen.
        Ok(Some(front).filter(|package| !package.is_empty()))
    }

    async fn output(&self) -> Result<String, String> {
        emulator_output(self.link, &self.id).await
    }
}
