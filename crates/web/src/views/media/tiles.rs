use leptos::prelude::*;
use stylance::classes;

use super::thumbnails::Thumbnails;
use super::tile::Tile;
use super::{Item, style};
use crate::ui;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::menu::MenuOf;
use crate::ui::pager::Pages;
use crate::ui::pick::Picking;

const EMPTY: &str =
    "The gallery is empty. Add photos or videos with the button, or drop them here.";

fn nothing(some: &bool) -> bool {
    !*some
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Tiles(
    total: Signal<Loadable<usize>>,
    items: Signal<Loadable<Vec<Item>>>,
    pages: Memo<Option<Pages>>,
    page: Memo<usize>,
    viewing: RwSignal<Option<String>>,
    picked: RwSignal<Vec<String>>,
    menu: MenuOf<Item>,
    thumbnails: Thumbnails,
    picking: Picking,
) -> impl IntoView {
    // The tiles are built again only when this changes, so they keep their
    // pictures when an item is added or deleted.
    let some = Memo::new(move |_| total.get().map(|all| all > 0));
    let tiles = move || match items.get() {
        Loadable::Ready(items) => items,
        Loadable::Loading | Loadable::Failed(_) => Vec::new(),
    };
    // As many placeholders as the page will have, so nothing shifts when it
    // arrives.
    let waiting = move || match pages.get() {
        Some(pages) if items.with(|items| matches!(items, Loadable::Loading)) => {
            pages.on_page(page.get())
        }
        _ => 0,
    };
    let failed = move || {
        items.with(|items| match items {
            Loadable::Failed(why) => Some(why.clone()),
            Loadable::Loading | Loadable::Ready(_) => None,
        })
    };

    view! {
        <Status of=some empty=(nothing, EMPTY) let:_some>
            {move || failed().map(|why| view! { <p class=ui::style::hint>{why}</p> })}
            <div class=style::tiles data-picks>
                <For
                    each=tiles
                    key=|item| (item.path.clone(), item.changed.clone())
                    children=move |item| {
                        view! { <Tile item viewing picked menu thumbnails picking /> }
                    }
                />
                {move || {
                    (0..waiting())
                        .map(|_| {
                            view! {
                                <figure class=style::tile aria-hidden="true">
                                    <div class=style::shown>
                                        <span></span>
                                        <figcaption>
                                            <span>"\u{a0}"</span>
                                            <span class=classes!(
                                                ui::style::hint, style::size
                                            )>"\u{a0}"</span>
                                        </figcaption>
                                    </div>
                                </figure>
                            }
                        })
                        .collect_view()
                }}
            </div>
        </Status>
    }
}
