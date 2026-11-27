use leptos::prelude::*;
use stylance::classes;

use super::thumbnails::Thumbnails;
use super::{Item, style};
use crate::app::{Icon, icons};
use crate::ui;
use crate::ui::menu::MenuOf;
use crate::ui::pick::{self, Picking, Tick};
use crate::util::size;

fn is(name: &str, kinds: &[&str]) -> bool {
    let name = name.to_lowercase();

    kinds.iter().any(|kind| name.ends_with(&format!(".{kind}")))
}

pub(super) fn is_picture(name: &str) -> bool {
    is(name, &["jpg", "jpeg", "png", "webp", "gif"])
}

pub(super) fn is_video(name: &str) -> bool {
    is(name, &["mp4", "webm", "mkv", "mov", "3gp"])
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Tile(
    item: Item,
    viewing: RwSignal<Option<String>>,
    picked: RwSignal<Vec<String>>,
    menu: MenuOf<Item>,
    thumbnails: Thumbnails,
    picking: Picking,
) -> impl IntoView {
    let name = item.name().to_owned();
    let bytes = size(item.size);
    let video = is_video(&name);
    let is_picked = pick::is_picked(picked, item.path.clone());
    let item = StoredValue::new(item);
    // A memo, so the tile redraws only when its own thumbnail arrives and
    // stays put for other batches.
    let picture = Memo::new(move |_| item.with_value(|item| thumbnails.address(item)));
    let pick = move || item.with_value(|item| pick::toggle(picked, &item.path));

    view! {
        <figure
            class=style::tile
            class=(style::picked, is_picked)
            data-pick=item.with_value(|item| item.path.clone())
            on:contextmenu=move |event| menu.at_pointer(&event, item.get_value())
        >
            <button
                type="button"
                class=style::shown
                aria-label=format!("Open {name} at full size")
                on:click=move |event| {
                    // The click that ends a drag is ignored.
                    if picking.swept() {
                    } else if event.ctrl_key() || event.meta_key() {
                        pick();
                    } else {
                        viewing.set(Some(item.with_value(|item| item.path.clone())));
                    }
                }
            >
                <span>
                    {move || match picture.get() {
                        Some(address) => view! { <img src=address alt="" /> }.into_any(),
                        None if video => "Video".into_any(),
                        None => "Photo".into_any(),
                    }}
                    {video.then(|| view! { <span class=style::video><Icon paths=icons::PLAY /></span> })}
                </span>
                <figcaption>
                    <span title=name.clone()>{name.clone()}</span>
                    <span class=classes!(ui::style::hint, style::size)>{bytes}</span>
                </figcaption>
            </button>
            <Tick
                picked=is_picked
                label=format!("Pick {name}")
                class=style::pick
                attr:title="Pick it"
                on:click=move |_| pick()
            />
        </figure>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_go_by_the_extension_in_any_case() {
        assert!(is_picture("IMG_0001.JPG"));
        assert!(is_video("clip.mp4"));
        assert!(!is_picture("clip.mp4"));
        assert!(!is_video("notes.txt"));
    }
}
