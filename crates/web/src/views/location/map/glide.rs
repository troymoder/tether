//! The map scales about the place under the pointer while the wheel turns,
//! and settles on the nearest whole zoom once the wheel rests.

use leptos::prelude::*;

#[derive(Clone, Copy)]
pub(super) struct Glide {
    /// The number of zoom levels closer than the current zoom the map is
    /// drawn at.
    pub(super) drawn: RwSignal<f64>,
    wanted: StoredValue<f64>,
    pub(super) still: RwSignal<(f64, f64)>,
    turned_at: StoredValue<f64>,
    going: StoredValue<bool>,
    /// Runs with the direction and the still point each time a whole zoom
    /// is passed.
    passed: Callback<(i8, (f64, f64))>,
}

pub(super) fn use_glide(passed: Callback<(i8, (f64, f64))>) -> Glide {
    Glide {
        drawn: RwSignal::new(0.0),
        wanted: StoredValue::new(0.0),
        still: RwSignal::new((0.0, 0.0)),
        turned_at: StoredValue::new(0.0),
        going: StoredValue::new(false),
        passed,
    }
}

impl Glide {
    /// `farthest` and `closest` are how many zooms the map may go from the
    /// current one, out and in.
    pub(super) fn turn(self, by: f64, about: (f64, f64), farthest: f64, closest: f64) {
        if !self.going.get_value() {
            self.still.set(about);
        }

        self.wanted.update_value(|wanted| {
            *wanted = (*wanted + by).clamp(farthest, closest);
        });

        self.turned_at.set_value(js_sys::Date::now());

        if !self.going.get_value() {
            self.going.set_value(true);
            request_animation_frame(move || self.step());
        }
    }

    fn step(self) {
        // The map is gone.
        let Some(drawn) = self.drawn.try_get_untracked() else {
            return;
        };

        let Some(mut wanted) = self.wanted.try_get_value() else {
            return;
        };

        // A wheel idle for 140 ms has rested, and the nearest whole zoom is
        // wanted.
        let rested = js_sys::Date::now() - self.turned_at.get_value() > 140.0;

        if rested {
            wanted = wanted.round();
            self.wanted.set_value(wanted);
        }

        let near = (wanted - drawn).abs() < 0.004;
        let drawn = if near {
            wanted
        } else {
            drawn + (wanted - drawn) * 0.3
        };

        // Each whole zoom reached becomes the current zoom, with the rest of
        // the way still to go.
        let whole = drawn.trunc();

        if whole != 0.0 {
            self.passed.run((whole as i8, self.still.get_untracked()));
            self.wanted.set_value(wanted - whole);
        }

        self.drawn.set(drawn - whole);

        if rested && near {
            self.going.set_value(false);
            return;
        }

        request_animation_frame(move || self.step());
    }
}
