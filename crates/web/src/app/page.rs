use std::time::Duration;

use futures_channel::oneshot;
use leptos::prelude::*;
use protocol::DeviceId;

use super::drop::Drops;
use super::panel::PANEL_KEY;
use super::storage::Read;
use super::theme::{THEME_KEY, Theme};
use crate::ui::kept::{Kept, KeptHere, kept_started};

#[derive(Clone, Copy)]
pub(crate) struct Ui {
    pub panel: KeptHere<Panel>,
    pub theme: ReadSignal<Theme>,
    kept_theme: KeptHere<Theme>,
    pub toast: RwSignal<Option<Notice>>,
    pub confirm: RwSignal<Option<Confirm>>,
    pub adding: RwSignal<Option<Adding>>,
    pub wanted: RwSignal<Option<Wanted>>,
    pub drops: Drops,
}

/// A device the user asked for before tetherd listed it, so before its id
/// was known. The router picks it once it is listed (app/route.rs).
#[derive(Clone, PartialEq)]
pub(crate) enum Wanted {
    /// By its virtual device's name.
    Emulator(String),
    Address(String),
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Panel {
    Auto,
    Open,
    Shut,
    Full,
}

impl Kept for Panel {
    fn text(&self) -> Option<String> {
        let text = match self {
            Panel::Auto => "auto",
            Panel::Open => "open",
            Panel::Shut => "shut",
            Panel::Full => "full",
        };

        Some(text.to_owned())
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(Panel::Auto),
            "open" => Some(Panel::Open),
            "shut" => Some(Panel::Shut),
            "full" => Some(Panel::Full),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct Notice {
    pub text: String,
    /// Milliseconds since the epoch. It tells this notice from a newer one
    /// when its timeout fires.
    shown: f64,
}

/// Dropping it answers no.
pub(crate) struct Confirm {
    pub title: String,
    pub body: String,
    pub action: &'static str,
    answer: oneshot::Sender<bool>,
}

#[derive(Clone, PartialEq)]
pub(crate) enum Adding {
    Any,
    Pairing(DeviceId, protocol::Route),
}

impl Ui {
    pub(crate) fn new(settings: &Read) -> Self {
        let kept_theme = kept_started(settings, THEME_KEY, Theme::System);

        kept_theme.get_untracked().apply();

        Self {
            panel: kept_started(settings, PANEL_KEY, Panel::Auto),
            theme: kept_theme.read_only(),
            kept_theme,
            toast: RwSignal::new(None),
            confirm: RwSignal::new(None),
            adding: RwSignal::new(None),
            wanted: RwSignal::new(None),
            drops: Drops::new(),
        }
    }

    pub(crate) fn recolour(self, theme: Theme) {
        // The page is coloured first, so what follows `theme` reads the new
        // colours from the computed style.
        theme.apply();
        self.kept_theme.set(theme);
    }

    pub(crate) fn say(self, text: impl Into<String>) {
        let shown = js_sys::Date::now();

        self.toast.set(Some(Notice {
            text: text.into(),
            shown,
        }));

        set_timeout(
            move || {
                // Only while no newer notice replaced it.
                self.toast.maybe_update(|toast| {
                    let mine = toast.as_ref().is_some_and(|toast| toast.shown == shown);

                    if mine {
                        *toast = None;
                    }

                    mine
                });
            },
            Duration::from_secs(6),
        );
    }

    /// `action` is the confirming button's label.
    pub(crate) async fn agreed(
        self,
        title: impl Into<String>,
        body: impl Into<String>,
        action: &'static str,
    ) -> bool {
        let (answer, answered) = oneshot::channel();

        // Replacing an earlier question drops its sender, which answers it
        // with no.
        self.confirm.set(Some(Confirm {
            title: title.into(),
            body: body.into(),
            action,
            answer,
        }));

        answered.await.unwrap_or(false)
    }

    /// `then` has to tell the user how it went by itself, as `Link::ask` and
    /// the run of a `use_action` do.
    pub(crate) fn confirm_then(
        self,
        title: impl Into<String>,
        body: impl Into<String>,
        action: &'static str,
        then: impl Future<Output = ()> + 'static,
    ) {
        let title = title.into();
        let body = body.into();

        leptos::task::spawn_local(async move {
            if self.agreed(title, body, action).await {
                then.await;
            }
        });
    }

    pub(crate) fn answer(self, agreed: bool) {
        if let Some(confirm) = self.confirm.try_update(Option::take).flatten() {
            let _ = confirm.answer.send(agreed);
        }
    }
}
