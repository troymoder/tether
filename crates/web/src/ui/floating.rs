//! A floating box is a `popover`, so it is fixed to the window whatever it is
//! in.

use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlDivElement, Node};

/// The room a box keeps from the window's edge, in pixels.
const EDGE: f64 = 8.0;

/// The room between a box and the button it hangs from, in pixels.
const GAP: f64 = 4.0;

pub(crate) fn goes_above(high: f64, above: f64, below: f64) -> bool {
    below < high && above > below
}

/// In window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Anchor {
    Point {
        x: f64,
        y: f64,
    },
    /// The box lines up with the button's left edge, or its right edge with
    /// `right`, and with `as_wide` is at least as wide.
    Button {
        left: f64,
        right_edge: f64,
        top: f64,
        bottom: f64,
        right: bool,
        as_wide: bool,
    },
}

impl Anchor {
    pub(crate) fn button(of: &web_sys::Element, right: bool, as_wide: bool) -> Self {
        let own = of.get_bounding_client_rect();

        Anchor::Button {
            left: own.left(),
            right_edge: own.right(),
            top: own.top(),
            bottom: own.bottom(),
            right,
            as_wide,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Place {
    left: f64,
    width: f64,
    side: Side,
    tallest: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    /// Below what it hangs from, this far from the window's top.
    Top(f64),
    /// Above it, this far from the window's bottom.
    Bottom(f64),
}

impl Place {
    fn style(&self) -> String {
        let Place {
            left,
            width,
            side,
            tallest,
        } = self;
        let side = match side {
            Side::Top(top) => format!("top: {top}px"),
            Side::Bottom(bottom) => format!("bottom: {bottom}px"),
        };

        format!("left: {left}px; width: {width}px; {side}; max-height: {tallest}px")
    }
}

pub(crate) fn place(window: (f64, f64), size: (f64, f64), anchor: Anchor) -> Place {
    let (wide, high) = window;
    let (asked, start, over, under) = match anchor {
        Anchor::Point { x, y } => (size.0, x, y, y),
        Anchor::Button {
            left,
            right_edge,
            top,
            bottom,
            right,
            as_wide,
        } => {
            let asked = if as_wide {
                size.0.max(right_edge - left)
            } else {
                size.0
            };
            let start = if right { right_edge - asked } else { left };

            (asked, start, top - GAP, bottom + GAP)
        }
    };
    let width = asked.min(wide - 2.0 * EDGE);
    let left = start.min(wide - width - EDGE).max(EDGE);
    let above = over - EDGE;
    let below = high - under - EDGE;

    if goes_above(size.1, above, below) {
        Place {
            left,
            width,
            side: Side::Bottom(high - over),
            tallest: above,
        }
    } else {
        Place {
            left,
            width,
            side: Side::Top(under),
            tallest: below,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Floating {
    boxed: NodeRef<html::Div>,
    /// None until the box has been measured.
    place: RwSignal<Option<Place>>,
}

/// `boxed` has to be a `popover="manual"` with
/// `style=move || floating.style()`.
pub(crate) fn use_floating(boxed: NodeRef<html::Div>) -> Floating {
    Floating {
        boxed,
        place: RwSignal::new(None),
    }
}

impl Floating {
    /// Until the box is measured it is hidden at the window's corner, where
    /// it can be laid out.
    pub(crate) fn style(self) -> String {
        match self.place.get() {
            Some(place) => place.style(),
            None => "left: 0; top: 0; visibility: hidden".to_owned(),
        }
    }

    /// The box is placed a frame from now, once it is in the page. `placed`
    /// is then called with it, as to move the focus into it.
    pub(crate) fn place(
        self,
        anchor: impl Fn() -> Option<Anchor> + 'static,
        placed: impl FnOnce(HtmlDivElement) + 'static,
    ) {
        self.place.set(None);

        request_animation_frame(move || {
            let Some(boxed) = self.boxed.try_get_untracked().flatten() else {
                return;
            };

            let Some(anchor) = anchor() else {
                return;
            };

            // The box only renders once it is shown, so it is measured
            // after this.
            let _ = boxed.show_popover();

            let size = |value: Result<wasm_bindgen::JsValue, _>| {
                value
                    .ok()
                    .and_then(|value| value.as_f64())
                    .unwrap_or_default()
            };
            let window = (size(window().inner_width()), size(window().inner_height()));
            let laid = boxed.get_bounding_client_rect();

            self.place
                .set(Some(place(window, (laid.width(), laid.height()), anchor)));
            placed(boxed);
        });
    }
}

/// `moved` is given the scrolled node, or None for the window. Scroll events
/// do not bubble, so they are caught in the capture phase.
pub(crate) fn close_on_scroll(
    moved: impl Fn(Option<&Node>) -> bool + 'static,
    close: impl Fn() + 'static,
) {
    let scrolled = Closure::<dyn Fn(web_sys::Event)>::new(move |event: web_sys::Event| {
        // A resize targets the window, which is not a node.
        let under = event.target().and_then(|at| at.dyn_into::<Node>().ok());

        if moved(under.as_ref()) {
            close();
        }
    });

    let listener: js_sys::Function = scrolled
        .as_ref()
        .unchecked_ref::<js_sys::Function>()
        .clone();
    let _ = window().add_event_listener_with_callback_and_bool("scroll", &listener, true);
    let _ = window().add_event_listener_with_callback("resize", &listener);
    let heard = StoredValue::new_local(Some((listener, scrolled)));

    on_cleanup(move || {
        heard.try_update_value(|heard| {
            if let Some((listener, _)) = heard.take() {
                let _ = window()
                    .remove_event_listener_with_callback_and_bool("scroll", &listener, true);
                let _ = window().remove_event_listener_with_callback("resize", &listener);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_stays_inside_the_window() {
        assert_eq!(
            place(
                (800.0, 600.0),
                (200.0, 100.0),
                Anchor::Point { x: 100.0, y: 50.0 }
            )
            .style(),
            "left: 100px; width: 200px; top: 50px; max-height: 542px"
        );
        assert_eq!(
            place(
                (800.0, 600.0),
                (200.0, 100.0),
                Anchor::Point { x: 750.0, y: 550.0 }
            )
            .style(),
            "left: 592px; width: 200px; bottom: 50px; max-height: 542px"
        );
    }

    #[test]
    fn a_box_hangs_from_its_button() {
        let button = |right, as_wide| Anchor::Button {
            left: 300.0,
            right_edge: 600.0,
            top: 100.0,
            bottom: 134.0,
            right,
            as_wide,
        };

        assert_eq!(
            place((800.0, 600.0), (200.0, 100.0), button(false, false)).style(),
            "left: 300px; width: 200px; top: 138px; max-height: 454px"
        );
        assert_eq!(
            place((800.0, 600.0), (200.0, 100.0), button(true, false)).style(),
            "left: 400px; width: 200px; top: 138px; max-height: 454px"
        );
        assert_eq!(
            place((800.0, 600.0), (200.0, 100.0), button(false, true)).style(),
            "left: 300px; width: 300px; top: 138px; max-height: 454px"
        );
    }

    #[test]
    fn a_box_goes_above_only_with_more_room_there() {
        assert!(!goes_above(100.0, 500.0, 400.0));
        assert!(goes_above(100.0, 500.0, 50.0));
        assert!(!goes_above(100.0, 40.0, 50.0));
    }
}
