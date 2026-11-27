use leptos::prelude::*;

use super::panes::{Panes, Whose};
use super::sift::Sift;

/// The milliseconds after an input in which a scroll counts as the user's.
const OWN: f64 = 1500.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    /// The sifting of the logs it was made in (`Panes::sifting`), which it
    /// ends with.
    sifting: u32,
    lines: usize,
}

#[derive(Clone, Copy)]
pub(super) struct Reading {
    panes: Panes,
    sift: Sift,
    pinned: StoredValue<bool>,
    /// The last pause. It is read through `paused`, which leaves out one of
    /// an earlier sifting.
    held: RwSignal<Option<Held>>,
    /// The number of new lines held back while paused, so the lines being
    /// read stay still. None while the log follows.
    pub(super) paused: Memo<Option<usize>>,
    /// The scroll position at the last scroll, to tell its direction.
    last_top: StoredValue<f64>,
    /// The time of the user's last scrolling input, once there was one.
    /// The log also scrolls itself, as when it drops its oldest lines, and
    /// that must not count as the user reading back.
    handled: StoredValue<Option<f64>>,
}

impl Reading {
    pub(super) fn new(panes: Panes, sift: Sift) -> Self {
        let sifting = panes.sifting;
        let held = RwSignal::new(None::<Held>);
        let paused = Memo::new(move |_| {
            let sifting = sifting.get();

            held.get()
                .filter(|held| held.sifting == sifting)
                .map(|held| held.lines)
        });

        Reading {
            panes,
            sift,
            pinned: StoredValue::new(true),
            held,
            paused,
            last_top: StoredValue::new(0.0),
            handled: StoredValue::new(None),
        }
    }

    pub(super) fn is_paused(self) -> bool {
        self.paused.with(Option::is_some)
    }

    /// Not tracked, and false once the view has gone.
    pub(super) fn is_paused_now(self) -> bool {
        self.paused.try_with_untracked(Option::is_some) == Some(true)
    }

    pub(super) fn pinned(self) -> bool {
        self.pinned.try_get_value() == Some(true)
    }

    /// Stops following the end without holding new lines back.
    pub(super) fn unpin(self) {
        self.pinned.set_value(false);
    }

    pub(super) fn pause(self) {
        if self.paused.with_untracked(Option::is_none) {
            self.held.set(Some(Held {
                sifting: self.panes.sifting.get_untracked(),
                lines: 0,
            }));
        }
    }

    pub(super) fn held(self, lines: usize) {
        self.held.update(|held| {
            if let Some(held) = held {
                held.lines += lines;
            }
        });
    }

    pub(super) fn resume(self) {
        self.held.set(None);
        self.show_latest();
    }

    /// It writes no signal, so the view's Effect may call it when the
    /// filters change.
    pub(super) fn show_latest(self) {
        let Panes { device, output, .. } = self.panes;

        self.pinned.set_value(true);
        // The log may be shorter now, which moves the view without the user
        // scrolling.
        self.last_top.set_value(0.0);

        self.sift.with(|filters| {
            device.update_value(|log| log.filter(filters));
            output.update_value(|log| log.filter(filters));
        });

        // Drawn once for the new height, so the end is where it scrolls to.
        self.panes.draw();
        self.panes.shown().1.scroll_to_end();
        self.panes.draw();
    }

    pub(super) fn drawn_after(self, whose: Whose) {
        if self.panes.whose.try_get_untracked() != Some(whose) {
            return;
        }

        self.panes.draw();

        if self.pinned() {
            self.panes.of(whose).1.scroll_to_end();
            self.panes.draw();
        }
    }

    /// Scrolls a pinned log to its end if it is short of it, which a resize
    /// can leave it.
    pub(super) fn keep_end(self) {
        let (_, frame) = self.panes.shown();

        if self.pinned() && frame.below() > 0.5 {
            frame.scroll_to_end();
            self.panes.draw();
        }
    }

    pub(super) fn by_hand(self) {
        self.handled.set_value(Some(js_sys::Date::now()));
    }

    /// Handles a scroll of the log in view. Scrolling up by hand unpins the
    /// log, and scrolling more than half a view from the end pauses it, so
    /// the lines being read stay. Scrolling back to the end pins it again.
    ///
    /// This goes by the direction of the scroll, since the log's own jumps
    /// to the end only go down, and a resize can leave it a little short of
    /// the end without the user scrolling.
    pub(super) fn scrolled(self) {
        let (_, frame) = self.panes.shown();

        let Some(scroller) = frame.scroller.get_untracked() else {
            return;
        };

        let top = scroller.scroll_top();
        let own = self
            .handled
            .get_value()
            .is_some_and(|handled| js_sys::Date::now() - handled < OWN);
        let up = own && top < self.last_top.get_value() - 1.0;

        self.last_top.set_value(top);

        let below = frame.below();

        if up {
            self.pinned.set_value(false);

            if below > f64::from(scroller.client_height()) / 2.0 {
                self.pause();
            }
        } else if below < frame.row() * 2.0 && !self.is_paused_now() {
            self.pinned.set_value(true);
        }

        self.panes.draw();
    }
}
