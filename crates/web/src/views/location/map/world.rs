use std::cmp::Ordering;
use std::time::Duration;

use leptos::prelude::*;

use super::glide::Glide;
use crate::views::location::tiles::{TILE, Tile, across, spot};

/// The zoom that shows the whole world.
pub(super) const FARTHEST: u8 = 2;
const CLOSE: u8 = 15;

#[derive(Clone, Copy)]
pub(in crate::views::location) struct World {
    pub(super) zoom: RwSignal<u8>,
    /// The frame's middle on the whole world's map at the current zoom.
    pub(super) middle: RwSignal<(f64, f64)>,
    /// A guess until the frame is laid out.
    pub(super) size: RwSignal<(f64, f64)>,
    /// The point everything is placed from, as a share of the world's width
    /// and height, which is the same point at every zoom. Browsers cannot
    /// place elements millions of pixels away, which the far side of the
    /// world is at a close zoom.
    pub(super) anchor: RwSignal<(f64, f64)>,
    /// Counts how often the anchor was set, so tiles placed from an older
    /// anchor are told apart.
    pub(super) placing: RwSignal<u32>,
    /// The place the dot was dragged or put, until the device has answered
    /// to being put there (`settle`).
    pub(super) moved: RwSignal<Option<(f64, f64)>>,
    /// True just after the map zoomed out. The previous zoom's tiles stay on
    /// top for a moment until the new ones have loaded.
    lingering: RwSignal<bool>,
    lingers: StoredValue<u32>,
}

pub(in crate::views::location) fn use_world(start: Option<(f64, f64)>) -> World {
    let zoom = if start.is_some() { CLOSE } else { FARTHEST };
    let (latitude, longitude) = start.unwrap_or((25.0, 10.0));
    let middle = spot(latitude, longitude, zoom);
    let across = across(zoom);

    World {
        zoom: RwSignal::new(zoom),
        middle: RwSignal::new(middle),
        size: RwSignal::new((600.0, 360.0)),
        anchor: RwSignal::new((middle.0 / across, middle.1 / across)),
        placing: RwSignal::new(0),
        moved: RwSignal::new(None),
        lingering: RwSignal::new(false),
        lingers: StoredValue::new(0),
    }
}

impl World {
    /// Changes zoom with the map point under a frame position staying under
    /// it. Nothing is placed anew, since each zoom's tiles stay. `closest`
    /// is the closest zoom the tile source has.
    pub(super) fn zoom_to(self, new: u8, closest: u8, x: f64, y: f64) {
        let old = self.zoom.get_untracked();
        let new = new.clamp(FARTHEST, closest);

        if new == old {
            return;
        }

        let scale = across(new) / across(old);
        let (wide, high) = self.size.get_untracked();
        let from_x = x - wide / 2.0;
        let from_y = y - high / 2.0;
        let (middle_x, middle_y) = self.middle.get_untracked();

        self.moved
            .update(|moved| *moved = moved.map(|(x, y)| (x * scale, y * scale)));
        self.middle.set((
            (middle_x + from_x) * scale - from_x,
            (middle_y + from_y) * scale - from_y,
        ));

        self.zoom.set(new);

        if new < old {
            self.lingering.set(true);
            self.lingers.update_value(|lingers| *lingers += 1);

            let this = self.lingers.get_value();

            set_timeout(
                move || {
                    if self.lingers.try_get_value() == Some(this) {
                        self.lingering.set(false);
                    }
                },
                Duration::from_millis(700),
            );
        }
    }

    /// Shows the map around a point at a zoom somewhere else entirely, with
    /// everything placed anew from there.
    pub(super) fn show(self, new_zoom: u8, new_middle: (f64, f64)) {
        let scale = across(new_zoom) / across(self.zoom.get_untracked());

        self.moved
            .update(|moved| *moved = moved.map(|(x, y)| (x * scale, y * scale)));
        self.anchor.set((
            new_middle.0 / across(new_zoom),
            new_middle.1 / across(new_zoom),
        ));

        self.placing.update(|placing| *placing += 1);
        self.middle.set(new_middle);
        self.zoom.set(new_zoom);
    }

    /// Shows a point in the part of the map the card leaves free: 166px
    /// right of the middle on a frame 560px wide or more, and about a
    /// quarter of its height lower on a narrower one, where the card lies on
    /// top.
    pub(super) fn focus(self, new_zoom: u8, (x, y): (f64, f64)) {
        let (wide, high) = self.size.get_untracked();
        let right = if wide < 560.0 { 0.0 } else { 166.0 };
        let down = if wide < 560.0 { 0.24 * high } else { 0.0 };

        self.show(new_zoom, (x - right, y - down));
    }

    pub(in crate::views::location) fn look_at(self, (latitude, longitude): (f64, f64)) {
        let zoom = self.zoom.get_untracked().max(CLOSE - 1);

        self.focus(zoom, spot(latitude, longitude, zoom));
    }

    pub(super) fn keep_in_view(self, (latitude, longitude): (f64, f64)) {
        let zoom = self.zoom.get_untracked();
        let (x, y) = spot(latitude, longitude, zoom);
        let (middle_x, middle_y) = self.middle.get_untracked();
        let (wide, high) = self.size.get_untracked();

        if (x - middle_x).abs() > wide / 2.0 - 24.0 || (y - middle_y).abs() > high / 2.0 - 24.0 {
            self.focus(zoom, (x, y));
        }
    }

    /// The device answered to being put somewhere, so the dot is drawn at
    /// the device's place again. The view may have closed by then.
    pub(in crate::views::location) fn settle(self) {
        self.moved.try_set(None);
    }

    /// The anchor in pixels at the current zoom. Tracked.
    pub(super) fn origin(self) -> (f64, f64) {
        let (x, y) = self.anchor.get();
        let across = across(self.zoom.get());

        (x * across, y * across)
    }

    /// The tiles a zoom needs: the ones in view, and for the closer zoom also
    /// the ones that will come into view. Tracked.
    pub(super) fn covering(self, level: u8, glide: Glide) -> Vec<Tile> {
        let at = self.zoom.get();
        let (wide, high) = self.size.get();
        let (middle_x, middle_y) = self.middle.get();
        let drawn = glide.drawn.get();
        let (still_x, still_y) = glide.still.get();

        // The scale the view is taken to be drawn at. It is smaller while
        // zooming out, which brings more into view, and 2 for the closer
        // zoom, which is what the map shows once it gets there or showed
        // before it zoomed out.
        let scale = match level.cmp(&at) {
            Ordering::Less => 2f64.powf(drawn.min(0.0)),
            Ordering::Equal => 1.0,
            Ordering::Greater if drawn > 0.01 || self.lingering.get() => 2.0,
            Ordering::Greater => return Vec::new(),
        };

        // The view at that scale about the still point, first at the current
        // zoom and then at this level.
        let from_x = still_x - wide / 2.0;
        let from_y = still_y - high / 2.0;
        let to_level = across(level) / across(at);
        let edge =
            |middle: f64, from: f64, half: f64| (middle + from + (half - from) / scale) * to_level;
        let column = |x: f64| (x / TILE).floor() as i64;
        let count = 1i64 << level;
        let mut tiles = Vec::new();

        let rows = column(edge(middle_y, from_y, -high / 2.0)).max(0)
            ..=column(edge(middle_y, from_y, high / 2.0)).min(count - 1);

        for y in rows {
            for x in column(edge(middle_x, from_x, -wide / 2.0))
                ..=column(edge(middle_x, from_x, wide / 2.0))
            {
                tiles.push((level, x, y, x.rem_euclid(count)));
            }
        }

        tiles
    }
}
