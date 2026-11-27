use leptos::prelude::*;

use super::device::Sources;
use super::frame::Frame;
use super::log::Log;
use super::sift::Sift;
use crate::app::{Global, Theme};
use crate::ui::find::Finding;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Whose {
    Device,
    Emulator,
}

impl Whose {
    /// A phone only has its own log, and an emulator that has not booted
    /// only its output. With both, the device's log shows until the user
    /// chooses.
    pub(super) fn shown(chosen: Option<Whose>, sources: Sources) -> Whose {
        if !sources.contains(Sources::OUTPUT) {
            Whose::Device
        } else if !sources.contains(Sources::DEVICE) {
            Whose::Emulator
        } else {
            chosen.unwrap_or(Whose::Device)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PickedLine {
    pub(super) number: u64,
    pub(super) whose: Whose,
    /// Its text, which may outlive its place in the log.
    pub(super) raw: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Searched {
    Missed,
    At {
        number: u64,
        /// Its place among the hits, from 1.
        at: usize,
        of: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Search {
    /// The sifting of the logs it was made in, which it ends with.
    sifting: u32,
    outcome: Searched,
}

/// The lines are kept outside signals and drawn on a terminal by `draw`, so
/// a change to them notifies no reader.
#[derive(Clone, Copy)]
pub(super) struct Panes {
    pub(super) device: StoredValue<Log, LocalStorage>,
    pub(super) output: StoredValue<Log, LocalStorage>,
    pub(super) device_frame: Frame,
    pub(super) output_frame: Frame,
    pub(super) whose: Memo<Whose>,
    /// Counts the siftings of the logs. It goes up each time a filter or
    /// the log in view changes. A search and a pause hold for the sifting
    /// they were made in, so neither has to be reset when it ends.
    pub(super) sifting: Memo<u32>,
    pub(super) finding: RwSignal<Finding<Search>>,
    /// None until a search is made, and again once the find bar closes or
    /// the lines are sifted anew.
    pub(super) searched: Memo<Option<Searched>>,
    pub(super) picked: RwSignal<Option<PickedLine>>,
    wrap: Signal<bool>,
    theme: ReadSignal<Theme>,
}

impl Panes {
    /// The colours follow the page's theme, so the view calls this as it is
    /// built.
    pub(super) fn new(whose: Memo<Whose>, sift: Sift, wrap: Signal<bool>) -> Self {
        let theme = expect_context::<Global>().ui.theme;
        let Sift {
            text, least, app, ..
        } = sift;
        let sifting = Memo::new(move |before: Option<&u32>| {
            text.track();
            least.track();
            app.track();
            whose.track();

            before.map_or(0, |before| before.wrapping_add(1))
        });
        let finding = RwSignal::new(Finding::<Search>::Closed);
        let searched = Memo::new(move |_| {
            let sifting = sifting.get();

            finding
                .get()
                .outcome()
                .filter(|search| search.sifting == sifting)
                .map(|search| search.outcome)
        });

        Panes {
            device: StoredValue::new_local(Log::default()),
            output: StoredValue::new_local(Log {
                emulator: true,
                ..Log::default()
            }),
            device_frame: Frame::new(),
            output_frame: Frame::new(),
            whose,
            sifting,
            finding,
            searched,
            picked: RwSignal::new(None),
            wrap,
            theme,
        }
    }

    pub(super) fn of(self, whose: Whose) -> (StoredValue<Log, LocalStorage>, Frame) {
        match whose {
            Whose::Device => (self.device, self.device_frame),
            Whose::Emulator => (self.output, self.output_frame),
        }
    }

    pub(super) fn shown(self) -> (StoredValue<Log, LocalStorage>, Frame) {
        self.of(self.whose.get_untracked())
    }

    pub(super) fn search(self, outcome: Option<Searched>) {
        let sifting = self.sifting.get_untracked();

        self.finding.update(|finding| {
            if let Finding::Open(search) = finding {
                *search = outcome.map(|outcome| Search { sifting, outcome });
            }
        });
    }

    pub(super) fn draw(self) {
        let whose = self.whose.get_untracked();
        let (log, frame) = self.of(whose);
        let found = match self.searched.get_untracked() {
            Some(Searched::At { number, .. }) => Some(number),
            Some(Searched::Missed) | None => None,
        };
        let picked = self.picked.with_untracked(|picked| {
            picked
                .as_ref()
                .filter(|picked| picked.whose == whose)
                .map(|picked| picked.number)
        });

        log.with_value(|log| {
            frame.draw(
                log,
                (found, picked),
                self.wrap.get_untracked(),
                self.theme.get_untracked(),
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_the_output_until_the_device_log_is_there() {
        let both = Sources::DEVICE | Sources::OUTPUT;

        assert_eq!(Whose::shown(None, Sources::OUTPUT), Whose::Emulator);
        assert_eq!(
            Whose::shown(Some(Whose::Device), Sources::OUTPUT),
            Whose::Emulator
        );
        assert_eq!(Whose::shown(Some(Whose::Emulator), both), Whose::Emulator);
        assert_eq!(Whose::shown(None, both), Whose::Device);
        assert_eq!(
            Whose::shown(Some(Whose::Emulator), Sources::DEVICE),
            Whose::Device
        );
    }
}
