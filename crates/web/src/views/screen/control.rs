use futures_channel::mpsc;
use leptos::prelude::*;

use super::playing::{Playing, Size};
use super::{Input, Turned};

/// It is made when the device is picked because the clipboard is read and set
/// through the screen, and the Clipboard view reads it.
#[derive(Clone, Copy)]
pub(crate) struct Control {
    /// The stream (stream.rs) is the only writer.
    pub(super) playing: RwSignal<Playing>,
    /// A memo, so its readers do not rerun when the video changes size or the
    /// screen rotates.
    live: Memo<bool>,
    pub(super) input: StoredValue<Option<mpsc::UnboundedSender<Input>>, LocalStorage>,
}

impl Control {
    pub(crate) fn new() -> Self {
        let playing = RwSignal::new(Playing::Idle);

        Self {
            playing,
            live: Memo::new(move |_| playing.with(Playing::live)),
            input: StoredValue::new_local(None),
        }
    }

    /// Tracked. False once another device is picked.
    pub(crate) fn live(self) -> bool {
        self.live.try_get().unwrap_or(false)
    }

    pub(super) fn turned(self) -> Option<Turned> {
        self.playing.with(Playing::turned)
    }

    pub(super) fn size(self) -> Option<Size> {
        self.playing
            .try_with_untracked(|playing| playing.live().then(|| playing.drawn()).flatten())
            .flatten()
    }

    pub(crate) fn send(self, input: Input) {
        self.input.try_with_value(|sender| {
            if let Some(sender) = sender {
                let _ = sender.unbounded_send(input);
            }
        });
    }
}
