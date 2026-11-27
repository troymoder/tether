use std::cell::Cell;
use std::time::Duration;

use leptos::prelude::*;

/// `run` gets the number of the tick, which starts at 1.
pub(super) fn use_tick(every: Duration, run: impl Fn(u32) + 'static) {
    let ticks = Cell::new(0u32);
    let ticking = set_interval_with_handle(
        move || {
            let tick = ticks.get().wrapping_add(1);

            ticks.set(tick);
            run(tick);
        },
        every,
    );

    if let Ok(ticking) = ticking {
        on_cleanup(move || ticking.clear());
    }
}
