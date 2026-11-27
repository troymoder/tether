use std::rc::Rc;

use leptos::ev;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::{Global, Page};
use crate::views::registry;

const APK: &str = "application/vnd.android.package-archive";

/// A type is empty where the browser cannot tell it. None when the drag holds
/// no files.
fn dragged(data: &web_sys::DataTransfer) -> Option<Vec<String>> {
    let items = data.items();
    let kinds: Vec<String> = (0..items.length())
        .filter_map(|index| items.get(index))
        .filter(|item| item.kind() == "file")
        .map(|item| item.type_())
        .collect();

    (!kinds.is_empty()).then_some(kinds)
}

/// A file without a type is accepted as a possible app and checked by its
/// name once dropped.
fn taken(page: &Page, kinds: &[String]) -> bool {
    match page {
        // Builds of every kind are told apart by their names once dropped.
        Page::Files { .. } | Page::Registry { .. } => true,
        Page::Media => kinds
            .iter()
            .all(|kind| kind.starts_with("image/") || kind.starts_with("video/")),
        _ => kinds.iter().all(|kind| kind.is_empty() || kind == APK),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Drops {
    /// Some while files from outside are over the views. True if the open
    /// view accepts them.
    pub(super) dragging: RwSignal<Option<bool>>,
    taker: StoredValue<Option<Taker>, LocalStorage>,
}

type Taker = Rc<dyn Fn(web_sys::File)>;

impl Drops {
    pub(super) fn new() -> Self {
        Drops {
            dragging: RwSignal::new(None),
            taker: StoredValue::new_local(None),
        }
    }

    pub(crate) fn take(self, take: impl Fn(web_sys::File) + 'static) {
        self.taker.set_value(Some(Rc::new(take)));
        on_cleanup(move || self.taker.set_value(None));
    }
}

pub(super) fn listen() {
    let Global { router, ui, .. } = expect_context::<Global>();
    let Drops { dragging, taker } = ui.drops;
    let upload = registry::use_upload();
    let onto_device = |event: &ev::DragEvent| {
        event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .and_then(|target| target.closest("#phone").ok().flatten())
            .is_some()
    };

    // True while a drag that started inside the page is under way, such as a
    // picture in Media. Those are never dropped here.
    let from_page = StoredValue::new(false);

    // The page lives as long as these listeners, so they are never removed.
    let _ = window_event_listener(ev::dragstart, move |_| from_page.set_value(true));
    let _ = window_event_listener(ev::dragend, move |_| from_page.set_value(false));
    let _ = window_event_listener(ev::dragover, move |event| {
        let Some(data) = event.data_transfer() else {
            return;
        };

        let kinds = dragged(&data);

        if from_page.get_value() || kinds.is_none() || onto_device(&event) {
            data.set_drop_effect("none");
            event.prevent_default();
            dragging.set(None);

            return;
        }

        event.prevent_default();

        let takes = kinds.is_some_and(|kinds| taken(&router.page_untracked(), &kinds));

        if !takes {
            data.set_drop_effect("none");
        }

        if dragging.get_untracked() != Some(takes) {
            dragging.set(Some(takes));
        }
    });

    let _ = window_event_listener(ev::dragleave, move |event| {
        // A dragleave fires for every element crossed. Only leaving the
        // window has no related target.
        if event.related_target().is_none() {
            dragging.set(None);
        }
    });

    let _ = window_event_listener(ev::drop, move |event| {
        event.prevent_default();

        let takes = dragging.get_untracked() == Some(true) && !onto_device(&event);

        dragging.set(None);

        if !takes {
            return;
        }

        let take = taker.get_value();

        if let Some(files) = event.data_transfer().and_then(|data| data.files()) {
            for index in 0..files.length() {
                if let Some(file) = files.get(index) {
                    match &take {
                        Some(take) => take(file),
                        None => upload.run(file),
                    }
                }
            }
        }
    });
}
