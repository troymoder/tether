//! Three zooms are drawn over each other: the current one, the farther one
//! under it, and while the zoom changes the closer one over it. A tile only
//! shows once it has loaded, so where one is missing the farther one under it
//! shows instead of a gap. A zoom's tiles stay in place when the map moves to
//! the next zoom, so nothing is fetched or drawn again for it.

use leptos::html;
use leptos::prelude::*;

use super::tiles::{KINDS, spot};
use crate::ui::kept::KeptHere;
use crate::ui::observe::use_resize;

mod controls;
mod glide;
mod layers;
mod pointer;
mod world;

use controls::Controls;
use glide::use_glide;
use layers::{Pins, Tiles};
use pointer::{use_pointer, zooms};
use world::FARTHEST;
pub(super) use world::{World, use_world};

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "map.module.scss"
);

/// The stored value is an index into `KINDS`.
pub(super) const KIND_KEY: &str = "map-kind";

#[allow(unreachable_pub)]
#[component]
pub(super) fn Map(
    #[prop(into)] at: Signal<Option<(f64, f64)>>,
    world: World,
    #[prop(into)] kept: Signal<Vec<protocol::Place>>,
    #[prop(into)] on_pick: Callback<(f64, f64)>,
    kind: KeptHere<u32>,
) -> impl IntoView {
    let frame = NodeRef::<html::Div>::new();
    // A stored index past the last source is the last one.
    let shown = Memo::new(move |_| (kind.get() as usize).min(KINDS.len() - 1));
    let closest = move || KINDS[shown.get_untracked()].closest;
    let glide = use_glide(Callback::new(move |(by, (x, y)): (i8, (f64, f64))| {
        world.zoom_to(
            (world.zoom.get_untracked() as i8 + by) as u8,
            closest(),
            x,
            y,
        );
    }));

    let turn = move |by: f64, x: f64, y: f64| {
        let now = f64::from(world.zoom.get_untracked());

        glide.turn(
            by,
            (x, y),
            f64::from(FARTHEST) - now,
            f64::from(closest()) - now,
        );
    };

    // The map only follows the dot when the dot would leave the view, as on
    // a route, so putting the dot leaves the map still. A route moves the
    // device from `Routes`, which plays on while this view is closed, so no
    // handler of the map hears of a step and an effect follows it.
    Effect::new(move |_| {
        if let Some(place) = at.get() {
            world.keep_in_view(place);
        }
    });

    use_resize(frame, move || {
        let Some(frame) = frame.get_untracked() else {
            return;
        };

        let wide = frame.client_width();
        let high = frame.client_height();

        // Zero until it has been laid out.
        if wide > 0 && high > 0 {
            world.size.set((f64::from(wide), f64::from(high)));
        }
    });

    let dot = Signal::derive(move || {
        world.moved.get().or_else(|| {
            let (latitude, longitude) = at.get()?;

            Some(spot(latitude, longitude, world.zoom.get()))
        })
    });

    let pointer = use_pointer(frame, world, dot, on_pick);
    let choose_kind = move |index: usize| {
        kind.set(index as u32);

        let closest = KINDS[index].closest;

        if world.zoom.get_untracked() > closest {
            let (wide, high) = world.size.get_untracked();

            world.zoom_to(closest, closest, wide / 2.0, high / 2.0);
        }
    };
    let centre = move |()| {
        if let Some((latitude, longitude)) = at.get_untracked() {
            let zoom = world.zoom.get_untracked();

            world.focus(zoom, spot(latitude, longitude, zoom));
        }
    };
    let zoom_by = move |by: f64| {
        let (wide, high) = world.size.get_untracked();

        turn(by, wide / 2.0, high / 2.0);
    };

    view! {
        <div
            node_ref=frame
            class=style::map
            class=(style::darkened, move || KINDS[shown.get()].darkened)
            on:pointerdown=move |event| pointer.down(&event)
            on:pointermove=move |event| pointer.moved(&event)
            on:pointerup=move |event| pointer.up(&event)
            on:pointercancel=move |_| pointer.cancel()
            on:wheel=move |event| {
                event.prevent_default();

                if let Some((x, y)) = pointer.within(event.client_x(), event.client_y()) {
                    turn(zooms(&event), x, y);
                }
            }
        >
            <div
                class=style::world
                style:transform=move || {
                    let (wide, high) = world.size.get();
                    let (middle_x, middle_y) = world.middle.get();
                    let (origin_x, origin_y) = world.origin();
                    let x = (wide / 2.0 - (middle_x - origin_x)).round();
                    let y = (high / 2.0 - (middle_y - origin_y)).round();
                    let (still_x, still_y) = glide.still.get();
                    let scale = 2f64.powf(glide.drawn.get());

                    format!(
                        "translate({still_x}px, {still_y}px) scale({scale}) translate({}px, {}px)",
                        x - still_x,
                        y - still_y,
                    )
                }
            >
                <Tiles world glide kind=shown />
                <Pins world glide kept dot />
            </div>
            <Controls
                kind=shown
                on_kind=choose_kind
                located=Signal::derive(move || at.with(Option::is_some))
                on_centre=centre
                on_zoom=zoom_by
            />
        </div>
    }
}
