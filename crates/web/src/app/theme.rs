//! base.scss switches its colour tokens on the root's `data-theme` attribute.

use leptos::prelude::*;

use crate::ui::kept::Kept;

pub(super) const THEME_KEY: &str = "theme";

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Theme {
    System,
    Light,
    Dark,
}

impl std::fmt::Display for Theme {
    fn fmt(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
        out.write_str(self.key())
    }
}

impl Kept for Theme {
    fn text(&self) -> Option<String> {
        Some(self.key().to_owned())
    }

    fn parse(text: &str) -> Option<Self> {
        Theme::ALL.into_iter().find(|theme| theme.key() == text)
    }
}

impl Theme {
    pub(crate) const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }

    fn attribute(self) -> Option<&'static str> {
        match self {
            Theme::System => None,
            Theme::Light => Some("light"),
            Theme::Dark => Some("dark"),
        }
    }

    fn key(self) -> &'static str {
        self.attribute().unwrap_or("system")
    }

    pub(super) fn is_light(self) -> bool {
        match self {
            Theme::Light => true,
            Theme::Dark => false,
            Theme::System => window()
                .match_media("(prefers-color-scheme: light)")
                .ok()
                .flatten()
                .is_some_and(|query| query.matches()),
        }
    }

    pub(super) fn apply(self) {
        if let Some(root) = document().document_element() {
            let _ = match self.attribute() {
                Some(name) => root.set_attribute("data-theme", name),
                None => root.remove_attribute("data-theme"),
            };
        }
    }
}
