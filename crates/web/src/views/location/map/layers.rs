use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::Element;

use super::glide::Glide;
use super::style;
use super::world::World;
use crate::views::location::tiles::{KINDS, TILE, Tile, across, address, spot};

fn picture(kind: usize, tile: Tile, anchor: (f64, f64)) -> impl IntoView {
    let (zoom, x, y, _) = tile;

    view! {
        <img
            src=address(kind, tile)
            alt=""
            draggable="false"
            style=format!(
                "left: {}px; top: {}px",
                (x as f64 * TILE - anchor.0 * across(zoom)).round(),
                (y as f64 * TILE - anchor.1 * across(zoom)).round(),
            )
            on:load=|event| {
                if let Some(tile) = event.target().and_then(|tile| tile.dyn_into::<Element>().ok()) {
                    let _ = tile.set_attribute("data-come", "");
                }
            }
        />
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Tiles(
    world: World,
    glide: Glide,
    #[prop(into)] kind: Signal<usize>,
) -> impl IntoView {
    // The zooms to draw, farthest first so it ends up underneath.
    let levels = move || {
        let kind = kind.get();
        let placing = world.placing.get();
        let at = world.zoom.get();

        (at.saturating_sub(1)..=(at + 1).min(KINDS[kind].closest))
            .map(|level| (kind, placing, level))
            .collect::<Vec<_>>()
    };

    view! {
        <For each=levels key=|level| *level let:level>
            {
                let (kind, _, level) = level;

                view! {
                    <div
                        class=style::level
                        style:transform=move || { format!("scale({})", across(world.zoom.get()) / across(level)) }
                    >
                        <For each=move || world.covering(level, glide) key=|tile| *tile let:tile>
                            {picture(kind, tile, world.anchor.get_untracked())}
                        </For>
                    </div>
                }
            }
        </For>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Pins(
    world: World,
    glide: Glide,
    #[prop(into)] kept: Signal<Vec<protocol::Place>>,
    #[prop(into)] dot: Signal<Option<(f64, f64)>>,
) -> impl IntoView {
    view! {
        // Drawn before the dot, so the dot is what gets grabbed where they
        // overlap.
        {move || {
            let (origin_x, origin_y) = world.origin();
            let zoom = world.zoom.get();
            let shrink = 2f64.powf(-glide.drawn.get());

            kept.get()
                .into_iter()
                .map(|place| {
                    let (x, y) = spot(place.latitude, place.longitude, zoom);

                    view! {
                        <div
                            class=style::kept
                            style=format!(
                                "left: {}px; top: {}px; transform: scale({shrink})",
                                x - origin_x,
                                y - origin_y,
                            )
                        >
                            <span>{place.name}</span>
                        </div>
                    }
                })
                .collect_view()
        }}
        {move || {
            dot.get()
                .map(|(x, y)| {
                    let (origin_x, origin_y) = world.origin();
                    let shrink = 2f64.powf(-glide.drawn.get());

                    view! {
                        <div
                            class=style::pin
                            title="Drag it to another place"
                            style=format!(
                                "left: {}px; top: {}px; transform: scale({shrink})",
                                x - origin_x,
                                y - origin_y,
                            )
                        ></div>
                    }
                })
        }}
    }
}
