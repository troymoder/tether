use std::collections::HashMap;

use leptos::prelude::*;

use super::front::{self, InFront};
use super::line::Level;
use super::log::Filters;
use crate::ui::loadable::Loadable;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Of {
    Front,
    All,
    App(String),
}

#[derive(Clone, Copy)]
pub(super) struct Sift {
    pub(super) text: RwSignal<String>,
    pub(super) least: RwSignal<Level>,
    pub(super) of: RwSignal<Of>,
    /// The package of the app in front. None on the home screen, and
    /// until the device has said.
    pub(super) front: Signal<Option<String>>,
    /// The package whose lines are shown. None shows every app's.
    pub(super) app: Memo<Option<String>>,
    pub(super) names: StoredValue<HashMap<u32, String>, LocalStorage>,
}

impl Sift {
    pub(super) fn new(in_front: Signal<Loadable<InFront>>) -> Self {
        let of = RwSignal::new(Of::Front);
        let front = Signal::derive(move || front::package(in_front));
        let app = Memo::new(move |_| match of.get() {
            Of::Front => front.get(),
            Of::All => None,
            Of::App(package) => Some(package),
        });

        Sift {
            text: RwSignal::new(String::new()),
            least: RwSignal::new(Level::Info),
            of,
            front,
            app,
            names: StoredValue::new_local(HashMap::new()),
        }
    }

    /// None once the view has gone. Not tracked.
    pub(super) fn with<T>(self, run: impl FnOnce(&Filters) -> T) -> Option<T> {
        let text = self.text.try_get_untracked()?.to_lowercase();
        let least = self.least.try_get_untracked()?;
        let app = self.app.try_get_untracked()?;

        self.names.try_with_value(|names| {
            run(&Filters {
                text,
                least,
                app,
                names,
            })
        })
    }
}
