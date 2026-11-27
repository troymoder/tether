use std::future::Future;
use std::time::Duration;

use leptos::prelude::*;

use crate::ui::action::use_action;
use crate::ui::debounced::use_debounced;
use crate::ui::loadable::Loadable;

const PAUSE: Duration = Duration::from_millis(300);

#[derive(Clone, Copy)]
pub(crate) struct Typed {
    pub(crate) text: RwSignal<String>,
    /// The text as it was when a path entered there led nowhere.
    missed: RwSignal<Option<String>>,
}

impl Typed {
    pub(crate) fn new() -> Self {
        Typed {
            text: RwSignal::new(String::new()),
            missed: RwSignal::new(None),
        }
    }

    pub(crate) fn missing(self) -> bool {
        self.missed.with(|missed| {
            missed
                .as_ref()
                .is_some_and(|typed| self.text.with(|text| text == typed))
        })
    }

    /// The view may be gone by the time this is called.
    pub(crate) fn clear(self) {
        self.missed.try_set(None);
        self.text.try_set(String::new());
    }

    pub(crate) fn use_found<T, Fut>(
        self,
        find: impl Fn(String) -> Fut + 'static,
    ) -> Signal<Option<Loadable<T>>>
    where
        T: Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<T, String>> + 'static,
    {
        let wanted = Signal::derive(move || {
            let wanted = self.text.get().trim().to_owned();

            (!wanted.is_empty()).then_some(wanted)
        });

        use_debounced(wanted, PAUSE, find)
    }

    /// `go` gets the path, trimmed and not empty, and answers true if it led
    /// somewhere.
    pub(crate) fn use_path<Fut>(self, go: impl Fn(String) -> Fut + 'static) -> Callback<String>
    where
        Fut: Future<Output = bool> + 'static,
    {
        let Typed { text, missed } = self;
        // The line under the bar says how it went, so the action says
        // nothing.
        let (_, enter) = use_action(None, move |path: String| {
            let typed = text.get_untracked();
            let going = go(path);

            async move {
                if !going.await {
                    // The view may be gone by now.
                    missed.try_set(Some(typed));
                }

                Ok(None::<String>)
            }
        });

        Callback::new(move |path: String| {
            let path = path.trim();

            if !path.is_empty() {
                enter.run(path.to_owned());
            }
        })
    }
}
