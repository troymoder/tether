use std::rc::Rc;

use leptos::prelude::*;

use super::device::Sources;
use super::panes::{Panes, Whose};
use super::reading::Reading;
use super::sift::Sift;
use crate::app::Picked;
use crate::device::Device;

#[derive(Clone, Copy)]
pub(super) struct OutputLog {
    /// The number of lines of the output that were cleared here. The
    /// output comes whole each time, so they are skipped.
    skipped: StoredValue<usize>,
    from: Signal<Rc<dyn Device>, LocalStorage>,
    panes: Panes,
    sift: Sift,
    reading: Reading,
}

pub(super) fn use_output_log(panes: Panes, sift: Sift, reading: Reading) -> OutputLog {
    OutputLog {
        skipped: StoredValue::new(0),
        from: expect_context::<Picked>().device,
        panes,
        sift,
        reading,
    }
}

impl OutputLog {
    pub(super) fn fetch(self) {
        let OutputLog {
            skipped,
            from,
            panes,
            sift,
            reading,
        } = self;

        let Some(followed) = from.try_get_untracked() else {
            return;
        };

        if !followed.sources().contains(Sources::OUTPUT) {
            return;
        }

        leptos::task::spawn_local(async move {
            let Ok(text) = followed.output().await else {
                return;
            };

            // Dropped if the view is gone.
            let Some(mut skip) = skipped.try_get_value() else {
                return;
            };

            // Fewer lines than were cleared means the emulator restarted
            // since.
            if text.lines().count() < skip {
                skip = 0;
                skipped.set_value(0);
            }

            let text: String = text
                .lines()
                .skip(skip)
                .flat_map(|line| [line, "\n"])
                .collect();
            let same = panes
                .output
                .with_value(|log| log.lines.iter().map(String::as_str).eq(text.lines()));

            if reading.is_paused_now() || same {
                return;
            }

            sift.with(|filters| {
                panes.output.update_value(|log| {
                    log.clear();
                    log.push(&text, Some(filters));
                });
            });

            reading.drawn_after(Whose::Emulator);
        });
    }

    pub(super) fn skip_shown(self) {
        let shown = self.panes.output.with_value(|log| log.lines.len());

        self.skipped.update_value(|skipped| *skipped += shown);
    }
}
