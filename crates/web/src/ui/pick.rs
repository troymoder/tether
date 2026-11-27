//! Only the mouse drags a box, because a finger dragging scrolls.
// shortcut: the list does not scroll by itself when the drag nears its edge,
// and there is no drag that unpicks. Add them if they are missed.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use leptos::ev;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};

use crate::ui::menu::Item;
use crate::util::counted;

mod bar;

pub(crate) use bar::{PickedBar, Tick};

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "pick.module.scss"
);

/// The distance a press moves before it becomes a drag, in pixels.
const SLACK: f64 = 5.0;

struct Sweep {
    from: (f64, f64),
    grid: Element,
    cells: Vec<Element>,
    hit: Vec<bool>,
    swept: &'static str,
    drawn: Option<HtmlElement>,
    picked: RwSignal<Vec<String>>,
}

#[derive(Clone, Copy)]
pub(crate) struct Picking(StoredValue<Drag, LocalStorage>);

#[derive(Default)]
struct Drag {
    sweep: RefCell<Option<Sweep>>,
    swept: Cell<bool>,
}

impl Picking {
    /// Listens over the whole window, since a drag leaves the view it
    /// started in.
    pub(crate) fn new() -> Self {
        let picking = Picking(StoredValue::new_local(Drag::default()));
        let moved = window_event_listener(ev::pointermove, move |event| picking.moved(&event));
        let up = window_event_listener(ev::pointerup, move |_| picking.ended());
        let cancelled = window_event_listener(ev::pointercancel, move |_| picking.ended());

        on_cleanup(move || {
            moved.remove();
            up.remove();
            cancelled.remove();
        });

        picking
    }

    fn with<T>(self, then: impl FnOnce(&Drag) -> T) -> Option<T> {
        self.0.try_with_value(then)
    }

    /// True when the click being handled is the one that ended a drag.
    pub(crate) fn swept(self) -> bool {
        self.with(|drag| drag.swept.get()) == Some(true)
    }

    fn moved(self, event: &ev::PointerEvent) {
        if event.buttons() == 0 {
            self.ended();
            return;
        }

        self.with(|drag| {
            let mut sweep = drag.sweep.borrow_mut();

            let Some(sweep) = &mut *sweep else {
                return;
            };

            let now = (event.client_x(), event.client_y());

            if sweep.drawn.is_none() {
                if (now.0 - sweep.from.0).hypot(now.1 - sweep.from.1) < SLACK {
                    return;
                }

                let drawn = document()
                    .create_element("div")
                    .ok()
                    .and_then(|drawn| drawn.dyn_into::<HtmlElement>().ok());

                if let Some(drawn) = &drawn {
                    drawn.set_class_name(style::sweep);
                    let _ = sweep.grid.append_child(drawn);
                }

                sweep.drawn = drawn;
            }

            let left = sweep.from.0.min(now.0);
            let right = sweep.from.0.max(now.0);
            let top = sweep.from.1.min(now.1);
            let bottom = sweep.from.1.max(now.1);
            let edge = sweep.grid.get_bounding_client_rect();

            if let Some(drawn) = &sweep.drawn {
                let style = web_sys::HtmlElement::style(drawn);

                let _ = style.set_property("left", &format!("{}px", left - edge.left()));
                let _ = style.set_property("top", &format!("{}px", top - edge.top()));
                let _ = style.set_property("width", &format!("{}px", right - left));
                let _ = style.set_property("height", &format!("{}px", bottom - top));
            }

            sweep.hit = sweep
                .cells
                .iter()
                .map(|cell| {
                    let at = cell.get_bounding_client_rect();
                    let on = at.left() < right
                        && at.right() > left
                        && at.top() < bottom
                        && at.bottom() > top;

                    let _ = cell.class_list().toggle_with_force(sweep.swept, on);

                    on
                })
                .collect();
        });
    }

    fn ended(self) {
        // Taken out first because picking may render the view again
        // and start another drag.
        let Some(sweep) = self.with(|drag| drag.sweep.take()).flatten() else {
            return;
        };

        let Some(drawn) = sweep.drawn else {
            return;
        };

        drawn.remove();

        let swept: Vec<String> = sweep
            .cells
            .iter()
            .zip(&sweep.hit)
            .filter_map(|(cell, hit)| {
                let _ = cell.class_list().remove_1(sweep.swept);

                hit.then(|| cell.get_attribute("data-pick")).flatten()
            })
            .collect();

        // The click that ends the drag lands on an item and must neither open
        // nor unpick it.
        self.with(|drag| drag.swept.set(true));
        set_timeout(
            move || {
                self.with(|drag| drag.swept.set(false));
            },
            Duration::ZERO,
        );

        sweep.picked.update(|picked| {
            for path in swept {
                if !picked.contains(&path) {
                    picked.push(path);
                }
            }
        });
    }

    /// The items are the elements with a `data-pick` naming them, inside the
    /// element with `data-picks`. The class `swept` marks an item the box
    /// touches, and the touched names are added to `picked`.
    pub(crate) fn start(
        self,
        event: &ev::PointerEvent,
        swept: &'static str,
        picked: RwSignal<Vec<String>>,
    ) {
        if event.pointer_type() != "mouse" || event.button() != 0 {
            return;
        }

        let element = |target: Option<web_sys::EventTarget>| target?.dyn_into::<Element>().ok();

        let Some(view) = element(event.current_target()) else {
            return;
        };

        let Some(pressed) = element(event.target()) else {
            return;
        };

        let within = |what: &str| pressed.closest(what).ok().flatten();

        // The view's own controls keep their presses unless they sit on an
        // item.
        if within("button, input, textarea, [role=toolbar], [role=listbox], [role=dialog]")
            .is_some()
            && within("[data-pick]").is_none()
        {
            return;
        }

        let Some(grid) = view.query_selector("[data-picks]").ok().flatten() else {
            return;
        };

        // A press on the scroll bar of the grid's scrolling parent is no drag.
        let on_bar = grid
            .parent_element()
            .filter(|scrolled| scrolled.contains(Some(&pressed)))
            .is_some_and(|scrolled| {
                let at = scrolled.get_bounding_client_rect();

                event.client_x() > at.left() + f64::from(scrolled.client_width())
                    || event.client_y() > at.top() + f64::from(scrolled.client_height())
            });

        if on_bar {
            return;
        }

        let cells = grid
            .query_selector_all("[data-pick]")
            .map(|found| {
                (0..found.length())
                    .filter_map(|at| found.get(at)?.dyn_into::<Element>().ok())
                    .collect()
            })
            .unwrap_or_default();

        self.with(|drag| {
            drag.sweep.replace(Some(Sweep {
                from: (event.client_x(), event.client_y()),
                grid,
                cells,
                hit: Vec::new(),
                swept,
                drawn: None,
                picked,
            }))
        });
    }
}

pub(crate) fn is_picked(picked: RwSignal<Vec<String>>, path: impl Into<String>) -> Signal<bool> {
    let path = path.into();

    Signal::derive(move || picked.with(|picked| picked.contains(&path)))
}

/// An item that is picked with others acts on all of them, and the heading
/// then counts them.
pub(crate) fn selection_menu(
    picked: RwSignal<Vec<String>>,
    path: String,
    name: String,
    download: Callback<Vec<String>>,
    delete: Option<Callback<Vec<String>>>,
) -> (String, Vec<Item>) {
    let is_picked = picked.with_untracked(|picked| picked.contains(&path));
    let together = is_picked && picked.with_untracked(|picked| picked.len() > 1);
    let paths = if together {
        picked.get_untracked()
    } else {
        vec![path.clone()]
    };
    let heading = if together { counted(paths.len()) } else { name };
    let saved = paths.clone();
    let mut items = vec![
        Item::Action(
            if is_picked { "Unselect" } else { "Select" },
            Callback::new(move |()| toggle(picked, &path)),
        ),
        Item::Action(
            "Download",
            Callback::new(move |()| download.run(saved.clone())),
        ),
    ];

    if let Some(delete) = delete {
        items.push(Item::Keyed {
            text: "Delete",
            keys: None,
            red: true,
            on: Callback::new(move |()| delete.run(paths.clone())),
        });
    }

    (heading, items)
}

pub(crate) fn toggle(picked: RwSignal<Vec<String>>, path: &str) {
    picked.update(
        |picked| match picked.iter().position(|other| other == path) {
            Some(at) => {
                picked.remove(at);
            }
            None => picked.push(path.to_owned()),
        },
    );
}
