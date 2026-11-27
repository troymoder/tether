use std::cell::Cell;
use std::rc::Rc;

use leptos::prelude::*;
use leptos::{ev, html};

use super::icons::{self, Icon};
use super::storage::Read;
use super::style;
use super::{Global, Panel};
use crate::ui::kept::{KeptHere, kept_started};

pub(super) const WIDTH_KEY: &str = "panel-width";
pub(super) const PANEL_KEY: &str = "panel";

const DEVICE: f64 = 440.0;
/// The least the panel gets before it lies over the device.
pub(super) const PANEL: f64 = 560.0;
const NARROWEST: f64 = 360.0;

/// `share` is where the panel was dragged between its narrowest and its
/// widest, 0 to 1, so that it keeps its place as the page changes size.
fn panel_width(share: Option<f64>, page: f64) -> f64 {
    let widest = (page - DEVICE).max(PANEL);
    let width = match share {
        // A share stored by something else may be out of range.
        Some(share) => NARROWEST + share.clamp(0.0, 1.0) * (widest - NARROWEST),
        None => widest,
    };

    width.max(NARROWEST).min(0.9 * page).round()
}

fn page_width() -> f64 {
    window()
        .inner_width()
        .ok()
        .and_then(|width| width.as_f64())
        .unwrap_or_default()
}

#[derive(Clone, Copy)]
struct Sizing {
    /// True when the pill was grabbed, where a press that hardly moves is a
    /// click.
    pill: bool,
    from: f64,
    /// The pointer's distance from the edge, kept through the drag so that
    /// the edge does not jump when it starts.
    offset: f64,
    next: Option<f64>,
}

#[derive(Clone, Copy)]
pub(super) struct Layout {
    page: RwSignal<f64>,
    /// The share `panel_width` takes.
    wanted: KeptHere<Option<f64>>,
    pub(super) full: Memo<bool>,
    pub(super) wide: Memo<f64>,
    pub(super) over: Memo<bool>,
    pub(super) shown: Memo<bool>,
}

impl Layout {
    pub(super) fn new(settings: &Read) -> Self {
        let ui = expect_context::<Global>().ui;
        let page = RwSignal::new(page_width());
        let wanted = kept_started(settings, WIDTH_KEY, None::<f64>);
        let full = Memo::new(move |_| ui.panel.get() == Panel::Full);
        let wide = Memo::new(move |_| {
            if full.get() {
                page.get()
            } else {
                panel_width(wanted.get(), page.get())
            }
        });

        let over = Memo::new(move |_| !full.get() && wide.get() > page.get() - DEVICE);
        let shown = Memo::new(move |_| match ui.panel.get() {
            Panel::Open | Panel::Full => true,
            Panel::Shut => false,
            Panel::Auto => !over.get(),
        });

        let _ = window_event_listener(ev::resize, move |_| page.set(page_width()));

        Layout {
            page,
            wanted,
            full,
            wide,
            over,
            shown,
        }
    }

    fn drags(self) -> bool {
        self.shown.get_untracked()
            && !self.over.get_untracked()
            && self.page.get_untracked() - DEVICE > NARROWEST
    }

    /// Callers check `drags` first, which holds the room divided by here to
    /// more than nothing.
    fn resize(self, next: f64) {
        let widest = self.page.get_untracked() - DEVICE;
        let share = ((next - NARROWEST) / (widest - NARROWEST)).clamp(0.0, 1.0);

        self.wanted.set(Some(share));
    }

    pub(super) fn sizes(self) -> String {
        let page = self.page.get();
        let wide = self.wide.get();
        let device = if !self.shown.get() {
            page
        } else if self.over.get() {
            page.min(DEVICE)
        } else {
            page - wide
        };

        format!("--p: {wide}px; --dev: {device}px")
    }
}

/// During a drag the widths are written straight onto `sides` and stored
/// when it ends, so that the page is not rendered again at every pointer
/// move.
#[allow(unreachable_pub)]
#[component]
pub(super) fn Edge(layout: Layout, sides: NodeRef<html::Div>) -> impl IntoView {
    let ui = expect_context::<Global>().ui;
    let sizing = Rc::new(Cell::new(None::<Sizing>));
    // True once a press on the pill became a drag, so the click it ends in
    // does not flip the panel.
    let dragged = Rc::new(Cell::new(false));

    let grab = {
        let sizing = Rc::clone(&sizing);

        move |event: &ev::PointerEvent, pill: bool| {
            if !layout.drags() {
                return;
            }

            let from = event.client_x();

            event.prevent_default();
            sizing.set(Some(Sizing {
                pill,
                from,
                offset: from - layout.wide.get_untracked(),
                next: None,
            }));
        }
    };

    let let_go = {
        let sizing = Rc::clone(&sizing);

        move || {
            let Some(held) = sizing.take() else {
                return;
            };

            if let Some(sides) = sides.get_untracked() {
                let _ = sides.class_list().remove_1(style::sizing);
            }

            if let Some(next) = held.next {
                layout.resize(next);
            }
        }
    };

    // The drag is followed on the window so that it carries on wherever the
    // pointer goes. It also ends when the buttons are found up, in case the
    // release was missed.
    let _ = window_event_listener(ev::pointermove, {
        let sizing = Rc::clone(&sizing);
        let dragged = Rc::clone(&dragged);
        let let_go = let_go.clone();

        move |event| {
            let Some(mut held) = sizing.get() else {
                return;
            };

            if event.buttons() == 0 {
                let_go();
                return;
            }

            let x = event.client_x();

            if held.pill && !dragged.get() {
                if (x - held.from).abs() < 4.0 {
                    return;
                }

                dragged.set(true);
            }

            let page = layout.page.get_untracked();
            let next = (x - held.offset).max(NARROWEST).min(page - DEVICE).round();

            if held.next == Some(next) {
                return;
            }

            held.next = Some(next);
            sizing.set(Some(held));

            if let Some(sides) = sides.get_untracked() {
                let style = web_sys::HtmlElement::style(&sides);

                let _ = sides.class_list().add_1(style::sizing);
                let _ = style.set_property("--p", &format!("{next}px"));
                let _ = style.set_property("--dev", &format!("{}px", page - next));
            }
        }
    });

    let _ = window_event_listener(ev::pointerup, {
        let let_go = let_go.clone();

        move |_| let_go()
    });

    let _ = window_event_listener(ev::pointercancel, move |_| let_go());

    // If hiding or showing the panel matches what Auto does at this page
    // width, it goes back to Auto so resizing the window keeps working
    // afterwards.
    let flip = {
        let dragged = Rc::clone(&dragged);

        move |_| {
            if dragged.replace(false) {
                return;
            }

            ui.panel.set(
                match (layout.shown.get_untracked(), layout.over.get_untracked()) {
                    (true, true) | (false, false) => Panel::Auto,
                    (true, false) => Panel::Shut,
                    (false, true) => Panel::Open,
                },
            );
        }
    };

    view! {
        <div
            class=style::grip
            class=(
                style::away,
                move || {
                    !(layout.shown.get() && !layout.over.get()
                        && layout.page.get() - DEVICE > NARROWEST)
                },
            )
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize the panel"
            tabindex="0"
            on:pointerdown={
                let grab = grab.clone();

                move |event| grab(&event, false)
            }
            on:dblclick=move |_| {
                layout.wanted.set(None);
            }
            on:keydown=move |event| {
                let step = match event.key().as_str() {
                    "ArrowLeft" => -16.0,
                    "ArrowRight" => 16.0,
                    _ => return,
                };

                if layout.drags() {
                    event.prevent_default();
                    layout
                        .resize(
                            (layout.wide.get_untracked() + step)
                                .max(NARROWEST)
                                .min(layout.page.get_untracked() - DEVICE),
                        );
                }
            }
        ></div>
        <button
            type="button"
            class=style::pill
            class=(style::away, move || !layout.shown.get())
            title="Show or hide the panel"
            aria-label="Show or hide the panel"
            on:pointerdown={
                let dragged = Rc::clone(&dragged);

                move |event| {
                    dragged.set(false);
                    grab(&event, true);
                }
            }
            on:click=flip
        >
            <Icon paths=icons::RIGHT />
        </button>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::kept::Kept;

    #[test]
    fn a_dragged_panel_keeps_its_share_as_the_page_changes() {
        // Beside the device there is room from 360 to page - 440.
        assert_eq!(panel_width(Some(0.0), 1600.0), 360.0);
        assert_eq!(panel_width(Some(1.0), 1600.0), 1160.0);
        assert_eq!(panel_width(Some(0.5), 1600.0), 760.0);
        assert_eq!(panel_width(Some(0.5), 2000.0), 960.0);
        assert_eq!(panel_width(None, 2000.0), 1560.0);
        assert_eq!(panel_width(None, 800.0), 560.0);
        assert_eq!(panel_width(Some(5.0), 1600.0), 1160.0);
        assert_eq!(panel_width(Some(-1.0), 1600.0), 360.0);
    }

    #[test]
    fn a_kept_panel_mode_reads_back() {
        for panel in [Panel::Auto, Panel::Open, Panel::Shut, Panel::Full] {
            let text = panel.text().expect("every mode has a text");

            assert!(Panel::parse(&text) == Some(panel));
        }

        assert!(Panel::parse("wide").is_none());
    }
}
