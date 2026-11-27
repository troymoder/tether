use leptos::prelude::*;
use leptos::{ev, html};
use wasm_bindgen::JsCast;
use web_sys::Element;

use super::style;
use super::world::World;
use crate::views::location::tiles::place;

#[derive(Clone, Copy)]
struct Held {
    x: f64,
    y: f64,
    /// The distance it has moved, which tells a press from a drag.
    gone: f64,
    has_dot: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Pointer {
    frame: NodeRef<html::Div>,
    world: World,
    dot: Signal<Option<(f64, f64)>>,
    on_pick: Callback<(f64, f64)>,
    held: StoredValue<Option<Held>>,
}

pub(super) fn use_pointer(
    frame: NodeRef<html::Div>,
    world: World,
    dot: Signal<Option<(f64, f64)>>,
    on_pick: Callback<(f64, f64)>,
) -> Pointer {
    Pointer {
        frame,
        world,
        dot,
        on_pick,
        held: StoredValue::new(None),
    }
}

fn inside(event: &ev::PointerEvent, class: &str) -> bool {
    event
        .target()
        .and_then(|target| target.dyn_into::<Element>().ok())
        .is_some_and(|target| {
            target
                .closest(&format!(".{class}"))
                .ok()
                .flatten()
                .is_some()
        })
}

impl Pointer {
    pub(super) fn within(self, x: f64, y: f64) -> Option<(f64, f64)> {
        let frame = self.frame.get_untracked()?;
        let bounds = frame.get_bounding_client_rect();

        Some((x - bounds.left(), y - bounds.top()))
    }

    pub(super) fn down(self, event: &ev::PointerEvent) {
        // The controls over the map take their own presses.
        if event.button() != 0 || inside(event, style::over) {
            return;
        }

        let has_dot = inside(event, style::pin);

        if let Some(frame) = self.frame.get_untracked() {
            let _ = frame.set_pointer_capture(event.pointer_id());
        }

        if has_dot {
            self.world.moved.set(self.dot.get_untracked());
        }

        self.held.set_value(Some(Held {
            x: event.client_x(),
            y: event.client_y(),
            gone: 0.0,
            has_dot,
        }));
    }

    pub(super) fn moved(self, event: &ev::PointerEvent) {
        let Some(Held {
            x,
            y,
            gone,
            has_dot,
        }) = self.held.get_value()
        else {
            return;
        };

        let now_x = event.client_x();
        let now_y = event.client_y();
        let by_x = now_x - x;
        let by_y = now_y - y;

        self.held.set_value(Some(Held {
            x: now_x,
            y: now_y,
            gone: gone + by_x.abs() + by_y.abs(),
            has_dot,
        }));

        if has_dot {
            self.world
                .moved
                .update(|moved| *moved = moved.map(|(x, y)| (x + by_x, y + by_y)));
        } else {
            self.world.middle.update(|(x, y)| {
                *x -= by_x;
                *y -= by_y;
            });
        }
    }

    pub(super) fn up(self, event: &ev::PointerEvent) {
        let Some(Held { gone, has_dot, .. }) = self.held.get_value() else {
            return;
        };

        let world = self.world;

        self.held.set_value(None);

        // A press without a drag puts the dot where it was pressed.
        let picked = if has_dot {
            world.moved.get_untracked()
        } else if gone < 4.0 {
            self.within(event.client_x(), event.client_y())
                .map(|(x, y)| {
                    let (wide, high) = world.size.get_untracked();
                    let (middle_x, middle_y) = world.middle.get_untracked();

                    (middle_x + x - wide / 2.0, middle_y + y - high / 2.0)
                })
        } else {
            None
        };

        if let Some((x, y)) = picked {
            world.moved.set(Some((x, y)));
            self.on_pick.run(place(x, y, world.zoom.get_untracked()));
        }

        // After a long drag away from the anchor, everything is placed
        // anew.
        let (middle_x, middle_y) = world.middle.get_untracked();
        let (origin_x, origin_y) = untrack(move || world.origin());

        if (middle_x - origin_x).abs() > 2e6 || (middle_y - origin_y).abs() > 2e6 {
            world.show(world.zoom.get_untracked(), (middle_x, middle_y));
        }
    }

    /// The browser took the pointer away. A dot that was held picks
    /// nothing, so it goes back to the device's place.
    pub(super) fn cancel(self) {
        if self.held.get_value().is_some_and(|held| held.has_dot) {
            self.world.moved.set(None);
        }

        self.held.set_value(None);
    }
}

/// The zooms a turn of the wheel asks for, closer when above 0.
pub(super) fn zooms(event: &ev::WheelEvent) -> f64 {
    // A mouse wheel notch is one zoom whatever delta the browser reports,
    // so the map never overshoots a zoom and comes back. A touchpad sends
    // many small deltas, which the map follows, and a touchpad pinch
    // arrives as small deltas with Control held, which count for more.
    let out = match (event.delta_mode(), event.ctrl_key()) {
        (_, true) => event.delta_y() / 70.0,
        (0, _) if event.delta_y().abs() < 50.0 => event.delta_y() / 200.0,
        _ => event.delta_y().signum(),
    };

    -out
}
