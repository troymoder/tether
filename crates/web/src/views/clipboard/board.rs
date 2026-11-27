use std::rc::Rc;
use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use protocol::DeviceId;

use crate::device::Device;
use crate::link::Link;
use crate::util::clock;
use crate::views::screen::Control;

const KEPT: usize = 12;
const COPIED_FOR: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Origin {
    Device,
    Computer,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Clip {
    /// `HH:MM:SS`.
    pub time: String,
    pub origin: Origin,
    pub text: String,
}

/// Made when the device is picked, since Follow goes on copying and the
/// screen's Copy button works with the Clipboard view closed. Follow ends
/// when another device is picked, since only the device's screen tells it
/// of a change (`told`), and that stream ends with the device's pane.
// Separate signals, so a reader of the text is not run again when only the
// recent clips or the copying change.
#[derive(Clone, Copy)]
pub(crate) struct Board {
    /// None until the device has sent it, which is not the same as an empty
    /// clipboard.
    pub text: RwSignal<Option<String>>,
    /// Newest first.
    pub clips: RwSignal<Vec<Clip>>,
    pub copying: RwSignal<Copying>,
    /// The timer that ends the moment in which the button that asked for a
    /// copy says "Copied". None outside that moment.
    copied: RwSignal<Option<TimeoutHandle>>,
    link: Link,
    screen: Control,
    device: Signal<Rc<dyn Device>, LocalStorage>,
    id: StoredValue<DeviceId>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Copying {
    Off,
    /// The device was asked for its clipboard, to copy it here once.
    Once,
    /// `once` while the device was also asked to copy it once, which confirms
    /// the copy even when the clipboard did not change.
    Following {
        once: bool,
    },
}

/// True if this browser writes the clipboard without a user's press for
/// each write. Chromium-based ones do while their tab has the focus.
pub(super) fn writes_unasked() -> bool {
    window()
        .navigator()
        .user_agent()
        .is_ok_and(|agent| agent.contains("Chrome"))
}

pub(super) fn write(text: &str) {
    let _ = window().navigator().clipboard().write_text(text);
}

impl Board {
    pub(crate) fn start(
        link: Link,
        screen: Control,
        device: Signal<Rc<dyn Device>, LocalStorage>,
        id: &DeviceId,
    ) -> Self {
        Board {
            text: RwSignal::new(None),
            clips: RwSignal::new(Vec::new()),
            copying: RwSignal::new(Copying::Off),
            copied: RwSignal::new(None),
            link,
            screen,
            device,
            id: StoredValue::new(id.clone()),
        }
    }

    /// The device's screen reports its clipboard when asked and on each
    /// change (scrcpy's clipboard_autosync). A stream that is still ending
    /// may call this after another device was picked, and then it does
    /// nothing.
    pub(crate) fn told(self, text: String) {
        let Some(copying) = self.copying.try_get_untracked() else {
            return;
        };

        let asked = matches!(copying, Copying::Once | Copying::Following { once: true });
        let changed = self
            .text
            .with_untracked(|before| before.as_ref() != Some(&text));

        if !text.is_empty() && (asked || (changed && matches!(copying, Copying::Following { .. })))
        {
            write(&text);

            if asked {
                self.link.noted(
                    "clipboard",
                    self.id.try_get_value().as_ref(),
                    Ok("Copied the device's clipboard".to_owned()),
                );
                self.confirm();
            }
        }

        if asked {
            self.copying.update(|copying| {
                *copying = match copying {
                    Copying::Off | Copying::Once => Copying::Off,
                    Copying::Following { .. } => Copying::Following { once: false },
                }
            });
        }

        if !changed {
            return;
        }

        // A clip put there from here is among the recent ones already, as
        // this computer's.
        let known = self
            .clips
            .with_untracked(|clips| clips.iter().any(|clip| clip.text == text));

        if !known {
            self.note(Origin::Device, &text);
        }

        self.text.set(Some(text));
    }

    fn confirm(self) {
        let hide = move || {
            self.copied.try_set(None);
        };
        let hiding = set_timeout_with_handle(hide, COPIED_FOR).ok();
        let before = self
            .copied
            .try_update(|copied| std::mem::replace(copied, hiding));

        if let Some(before) = before.flatten() {
            before.clear();
        }
    }

    pub(crate) fn copied(self) -> bool {
        self.copied.with(Option::is_some)
    }

    pub(super) fn note(self, origin: Origin, text: &str) {
        if text.trim().is_empty() {
            return;
        }

        self.clips.update(|clips| {
            clips.retain(|clip| clip.text != text);
            clips.insert(
                0,
                Clip {
                    time: clock(js_sys::Date::now())[..8].to_owned(),
                    origin,
                    text: text.to_owned(),
                },
            );

            clips.truncate(KEPT);
        });
    }

    pub(super) fn ask(self, text: Option<String>) {
        let Board { link, screen, .. } = self;
        let shown = untrack(|| screen.live());

        let Some(device) = self.device.try_get_untracked().filter(|_| shown) else {
            return;
        };

        spawn_local(async move {
            let written = match text {
                Some(text) => device.write_clipboard(screen, text).await,
                None => Ok(()),
            };

            if let Err(error) = written.and(device.read_clipboard(screen).await) {
                link.say(format!("clipboard failed: {error}"));
            }
        });
    }

    pub(super) fn put(self, origin: Origin, text: String) {
        self.note(origin, &text);
        // Shown as the device's until the device answers.
        self.text.set(Some(text.clone()));
        self.ask(Some(text));
    }

    pub(crate) fn take(self) {
        self.copying.update(|copying| {
            *copying = match copying {
                Copying::Off | Copying::Once => Copying::Once,
                Copying::Following { .. } => Copying::Following { once: true },
            }
        });

        self.ask(None);
    }

    pub(super) fn follow(self, on: bool) {
        self.copying.update(|copying| {
            let once = matches!(copying, Copying::Once | Copying::Following { once: true });

            *copying = match (on, once) {
                (true, once) => Copying::Following { once },
                (false, true) => Copying::Once,
                (false, false) => Copying::Off,
            }
        });
    }

    pub(super) fn following(self) -> bool {
        self.copying
            .with(|copying| matches!(copying, Copying::Following { .. }))
    }
}
