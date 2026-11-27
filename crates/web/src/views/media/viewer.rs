use leptos::html;
use leptos::prelude::*;
use stylance::classes;

use super::{Item, style};
use crate::app::{Icon, icons};
use crate::ui;
use crate::ui::button::{self, Button, Tone};
use crate::ui::pick;
use crate::util::size;

mod download;
mod progress;
mod seen;

use download::{Download, use_download};
use seen::Seen;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Viewed {
    item: Item,
    at: usize,
    of: usize,
    before: Option<String>,
    after: Option<String>,
}

impl Viewed {
    pub(super) fn among(items: &[Item], path: &str) -> Option<Self> {
        let at = items.iter().position(|item| item.path == path)?;
        let beside = |at: Option<usize>| Some(items.get(at?)?.path.clone());

        Some(Viewed {
            item: items[at].clone(),
            at,
            of: items.len(),
            before: beside(at.checked_sub(1)),
            after: beside(at.checked_add(1)),
        })
    }

    /// Stepping to another item, or the item moving on its page, builds a new
    /// viewer, so no handler holds on to a stale item.
    pub(super) fn key(&self) -> (String, usize, usize, Option<String>, Option<String>) {
        (
            self.item.path.clone(),
            self.at,
            self.of,
            self.before.clone(),
            self.after.clone(),
        )
    }
}

// `PopBox` would take every press on the window with its backdrop. Here only
// the dimmed gallery closes the viewer, so the menu and the device beside the
// view still take presses.
#[allow(unreachable_pub)]
#[component]
pub(super) fn Viewer(
    viewed: Viewed,
    viewing: RwSignal<Option<String>>,
    picked: RwSignal<Vec<String>>,
    scroller: NodeRef<html::Div>,
) -> impl IntoView {
    let Viewed {
        item,
        at,
        of,
        before,
        after,
    } = viewed;
    let close = NodeRef::<html::Button>::new();
    // Placed at the current scroll position, so the gallery does not jump
    // under it.
    let scrolled = scroller
        .get_untracked()
        .map_or(0.0, |scroller| scroller.scroll_top());
    let name = item.name().to_owned();
    let about = format!("{} · {} of {of} on this page", size(item.size), at + 1);
    let Download { fetched, run } = use_download(&item);
    let first = before.is_none();
    let last = after.is_none();
    let beside = StoredValue::new((before, after));
    let step = move |forward: bool| {
        let next = beside.with_value(|(before, after)| {
            if forward {
                after.clone()
            } else {
                before.clone()
            }
        });

        if next.is_some() {
            viewing.set(next);
        }
    };

    close.on_load(|close| {
        let _ = close.focus();
    });

    let mine = StoredValue::new(item.path.clone());
    let is_picked = move || picked.with(|picked| mine.with_value(|mine| picked.contains(mine)));
    let step_class = classes!(button::style::icon, button::style::framed, style::step);

    view! {
        <div
            class=style::viewer
            style=format!("top: {scrolled}px")
            on:keydown=move |event| match event.key().as_str() {
                "Escape" => {
                    // The Escape handler in app.rs skips prevented keys, so
                    // the panel over the device stays open.
                    event.prevent_default();
                    viewing.set(None);
                }
                "ArrowLeft" => step(false),
                "ArrowRight" => step(true),
                _ => {}
            }
        >
            <button
                type="button"
                class=style::around
                title="Close"
                aria-label="Close"
                on:click=move |_| viewing.set(None)
            ></button>
            <div role="dialog" aria-label=format!("{name}, at full size")>
                <header>
                    <button
                        node_ref=close
                        type="button"
                        class=classes!(button::style::button, style::act)
                        title="Close · Esc"
                        aria-label="Back to all the media"
                        on:click=move |_| viewing.set(None)
                    >
                        <Icon paths=icons::CLOSE />
                        "Close"
                    </button>
                    <span>
                        <code>{name.clone()}</code>
                        <span class=classes!(ui::style::hint, style::about)>{about}</span>
                    </span>
                    <Button
                        class=style::act
                        attr:title="Download it to this computer"
                        busy=Signal::derive(move || fetched.with(Option::is_some))
                        on:click=move |_| run.run(())
                    >
                        "Download"
                    </Button>
                    <Button
                        tone=Signal::derive(move || if is_picked() { Tone::Blue } else { Tone::Plain })
                        class=style::act
                        attr:aria-pressed=move || is_picked().to_string()
                        on:click=move |_| mine.with_value(|mine| pick::toggle(picked, mine))
                    >
                        {move || if is_picked() { "Picked" } else { "Pick" }}
                    </Button>
                </header>
                <div class=style::seen>
                    <button
                        type="button"
                        class=step_class.clone()
                        title="Before · Left arrow"
                        aria-label="The one before"
                        disabled=first
                        on:click=move |_| step(false)
                    >
                        <Icon paths=icons::BACK />
                    </button>
                    <div>
                        <Seen item download=fetched />
                    </div>
                    <button
                        type="button"
                        class=step_class
                        title="After · Right arrow"
                        aria-label="The one after"
                        disabled=last
                        on:click=move |_| step(true)
                    >
                        <Icon paths=icons::RIGHT />
                    </button>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str) -> Item {
        Item {
            path: path.to_owned(),
            size: 1,
            changed: "0".to_owned(),
        }
    }

    #[test]
    fn an_item_is_found_with_the_ones_beside_it() {
        let items = [item("/a.jpg"), item("/b.jpg"), item("/c.jpg")];
        let first = Viewed::among(&items, "/a.jpg").expect("it is on the page");
        let last = Viewed::among(&items, "/c.jpg").expect("it is on the page");

        assert_eq!((first.at, first.of), (0, 3));
        assert_eq!(first.before, None);
        assert_eq!(first.after.as_deref(), Some("/b.jpg"));
        assert_eq!(last.before.as_deref(), Some("/b.jpg"));
        assert_eq!(last.after, None);
        assert_eq!(Viewed::among(&items, "/d.jpg"), None);
    }
}
