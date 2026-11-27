//! The choices carry values of the view's own type, and nothing chosen is
//! `None`, never an empty string.

use leptos::prelude::*;
use leptos::{ev, html};
use stylance::classes;
use wasm_bindgen::JsCast;
use web_sys::{Element, Node};

use crate::app::{Icon, icons};
use crate::ui;
use crate::ui::floating::goes_above;
use crate::ui::tag::Dot;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "dropdown.module.scss"
);

const SEARCHED: usize = 8;

/// A row with the gap after it, in pixels.
const ROW: f64 = 35.0;

/// Four rows, matching `max-height` in dropdown.module.scss.
const TALLEST: f64 = 147.0;

/// The room the list keeps from the edge of its container, in pixels.
const GAP: f64 = 12.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Choice<T> {
    pub value: T,
    pub label: String,
    /// The state in words is for screen readers.
    pub dot: Option<(Dot, &'static str)>,
    pub disabled: bool,
    pub note: Option<String>,
    pub detail: Option<String>,
}

impl<T> Choice<T> {
    pub(crate) fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            dot: None,
            disabled: false,
            note: None,
            detail: None,
        }
    }

    pub(crate) fn noted(self, note: impl Into<String>) -> Self {
        Self {
            note: Some(note.into()),
            ..self
        }
    }
}

struct Open {
    up: bool,
    room: Option<f64>,
    at: usize,
    sought: String,
}

/// Returns true if the list opens upwards, and its height limit.
fn placement(above: f64, below: f64, rows: usize, searched: bool) -> (bool, f64) {
    let search = if searched { ROW } else { 0.0 };
    let wanted = (rows as f64 * ROW + search + 12.0).min(TALLEST);
    let up = goes_above(wanted, above, below);
    let room = if up { above } else { below };

    (up, room.max(ROW * 2.0))
}

/// The list stays inside its dialog, or else the window, so that opening it
/// never makes either scroll.
fn space(whole: &Element) -> (f64, f64) {
    let own = whole.get_bounding_client_rect();
    let window_height = window()
        .inner_height()
        .ok()
        .and_then(|height| height.as_f64())
        .unwrap_or_default();
    let popup = whole.closest("dialog, [popover]").ok().flatten();
    // A box from popup.rs is a popover, and its lists may reach past it up
    // to the window.
    let boxed = popup
        .as_ref()
        .is_some_and(|within| within.has_attribute("popover"));
    let (top, bottom) = popup
        .filter(|_| !boxed)
        .map(|within| {
            let within = within.get_bounding_client_rect();

            (within.top(), within.bottom())
        })
        .unwrap_or((0.0, window_height));

    (own.top() - top - GAP, bottom - own.bottom() - GAP)
}

fn highlight(open: RwSignal<Option<Open>>, to: usize) {
    open.update(|open| {
        if let Some(open) = open {
            open.at = to;
        }
    });
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Dropdown<T>(
    #[prop(into)] choices: Signal<Vec<Choice<T>>>,
    #[prop(into)] chosen: Signal<Option<T>>,
    #[prop(into)] on_choose: Callback<T>,
    label: &'static str,
    #[prop(optional)] id: Option<&'static str>,
    #[prop(optional)] class: &'static str,
    #[prop(optional, into)] placeholder: MaybeProp<&'static str>,
    #[prop(optional)] icon: Option<&'static str>,
    #[prop(optional, into)] disabled: Signal<bool>,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let whole = NodeRef::<html::Div>::new();
    let search = NodeRef::<html::Input>::new();
    let button = NodeRef::<html::Button>::new();
    let open = RwSignal::new(None::<Open>);
    // Each part of the open list by itself, so moving the highlight does not
    // build the rows again.
    let is_open = Memo::new(move |_| open.with(Option::is_some));
    let up = Memo::new(move |_| open.with(|open| open.as_ref().is_some_and(|open| open.up)));
    let at = Memo::new(move |_| open.with(|open| open.as_ref().map(|open| open.at)));
    let sought =
        Memo::new(move |_| open.with(|open| open.as_ref().map(|open| open.sought.to_lowercase())));

    let searched = move || choices.with(|choices| choices.len() >= SEARCHED);
    let listed = move || {
        let sought = sought.get();

        choices.with(|choices| {
            choices
                .iter()
                .filter(|choice| {
                    sought
                        .as_ref()
                        .is_none_or(|sought| choice.label.to_lowercase().contains(sought))
                })
                .cloned()
                .collect::<Vec<_>>()
        })
    };

    let current = move || {
        let chosen = chosen.get()?;

        choices.with(|choices| {
            choices
                .iter()
                .find(|choice| choice.value == chosen)
                .cloned()
        })
    };

    // The focus goes back to the button so keyboard use carries on from it.
    let close = move || {
        if let Some(button) = button.get_untracked() {
            let _ = button.focus();
        }

        open.set(None);
    };

    let choose = move |choice: Option<Choice<T>>| {
        if let Some(choice) = choice.filter(|choice| !choice.disabled) {
            on_choose.run(choice.value);
            close();
        }
    };

    let take = Callback::new(move |choice: Choice<T>| choose(Some(choice)));
    let highlighted = move || {
        let at = at.get_untracked()?;

        listed().into_iter().nth(at)
    };

    let show = move || {
        let chosen = chosen.get_untracked();
        let current = choices
            .with_untracked(|choices| {
                choices
                    .iter()
                    .position(|choice| Some(&choice.value) == chosen.as_ref())
            })
            .unwrap_or(0);
        let fits = whole.get_untracked().map(|whole| {
            let (above, below) = space(&whole);
            let rows = choices.with_untracked(Vec::len);

            placement(above, below, rows, untrack(searched))
        });

        open.set(Some(Open {
            up: fits.is_some_and(|(up, _)| up),
            room: fits.map(|(_, room)| room).filter(|room| *room < TALLEST),
            at: current,
            sought: String::new(),
        }));

        request_animation_frame(move || {
            if let Some(search) = search.get_untracked() {
                let _ = search.focus();
            }
        });
    };

    // Handled on the whole dropdown so the keys work wherever the focus is
    // inside it.
    let keys = move |event: ev::KeyboardEvent| {
        if !is_open.get_untracked() {
            if matches!(event.key().as_str(), "ArrowDown" | "ArrowUp") {
                event.prevent_default();
                show();
            }

            return;
        }

        let count = untrack(listed).len();

        match event.key().as_str() {
            key @ ("ArrowDown" | "ArrowUp") => {
                event.prevent_default();

                if count > 0 {
                    let step = if key == "ArrowDown" { 1 } else { count - 1 };
                    let now = at.get_untracked().unwrap_or(0);

                    highlight(open, (now + step) % count);
                }
            }
            // On the button the browser turns Enter into a click, handled
            // there. In the search box it is handled here.
            "Enter" if untrack(searched) => {
                event.prevent_default();
                choose(untrack(highlighted));
            }
            "Escape" => {
                // The dialog around the list must not close too.
                event.prevent_default();
                event.stop_propagation();
                close();
            }
            _ => {}
        }
    };

    let pressed = move |event: ev::MouseEvent| {
        if !is_open.get_untracked() {
            show();
        } else if event.detail() == 0 {
            // A click from Enter or Space has no detail. It takes the
            // highlighted choice.
            choose(untrack(highlighted));
        } else {
            open.set(None);
        }
    };

    view! {
        <div
            node_ref=whole
            class=move || classes!(style::dropdown, up.get().then_some(style::up), class)
            on:keydown=keys
            // A press anywhere else moves the focus out and closes the list.
            on:focusout=move |event| {
                // The focus also leaves when the dropdown is removed from the
                // page, and then there is nothing to close.
                let Some(whole) = whole.try_get_untracked().flatten() else {
                    return;
                };

                let to = event.related_target().and_then(|to| to.dyn_into::<Node>().ok());

                if !to.is_some_and(|to| whole.contains(Some(&to))) {
                    open.try_set(None);
                }
            }
        >
            <button
                node_ref=button
                type="button"
                class=style::trigger
                id=id
                disabled=move || disabled.get()
                aria-haspopup="listbox"
                aria-expanded=move || is_open.get().to_string()
                aria-label=move || match current() {
                    Some(Choice { label: chosen, dot: Some((_, state)), .. }) => {
                        format!("{label}: {chosen}, {state}")
                    }
                    Some(choice) => format!("{label}: {}", choice.label),
                    None => format!("{label}: none"),
                }
                // Safari and Firefox on macOS do not focus a pressed button.
                // The focus would leave the list and close it, and the click
                // would open it again, so the button is focused here.
                on:mousedown=move |event| {
                    event.prevent_default();

                    if let Some(button) = button.get_untracked() {
                        let _ = button.focus();
                    }
                }
                on:click=pressed
            >
                {move || {
                    let choice = current();
                    let dot = choice.as_ref().and_then(|choice| choice.dot);
                    let label = match choice {
                        Some(choice) => Some(choice.label),
                        None => placeholder.get().map(str::to_owned),
                    };

                    view! {
                        {dot.map(|(dot, _)| view! { <span class=dot.class()></span> })}
                        <span class=style::name>{label}</span>
                    }
                }}
                {icon.map(|paths| view! { <span class=style::sign><Icon paths /></span> })}
                <Icon paths=icons::CHEVRON />
            </button>
            <Show when=move || is_open.get()>
                <List
                    label
                    open
                    at
                    listed=Signal::derive(listed)
                    chosen
                    searched=Signal::derive(searched)
                    search
                    on_take=take
                />
            </Show>
        </div>
    }
}

#[component]
fn List<T>(
    label: &'static str,
    open: RwSignal<Option<Open>>,
    at: Memo<Option<usize>>,
    listed: Signal<Vec<Choice<T>>>,
    chosen: Signal<Option<T>>,
    searched: Signal<bool>,
    search: NodeRef<html::Input>,
    on_take: Callback<Choice<T>>,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    view! {
        <div
            class=style::list
            style=move || {
                open.with(|open| open.as_ref().and_then(|open| open.room))
                    .map(|room| format!("max-height: {room}px"))
            }
        >
            <Show when=move || searched.get()>
                <input
                    node_ref=search
                    type="search"
                    placeholder="Search"
                    aria-label=format!("Search: {label}")
                    // The filtered list changes with each letter, so the
                    // highlight goes back to its first row.
                    on:input=move |event| {
                        let typed = event_target_value(&event);

                        open.update(|open| {
                            if let Some(open) = open {
                                open.sought = typed;
                                open.at = 0;
                            }
                        });
                    }
                />
            </Show>
            <ul role="listbox" aria-label=label>
                {move || {
                    let chosen = chosen.get();
                    let listed = listed.get();

                    if listed.is_empty() {
                        return view! { <li class=ui::style::hint>"Nothing like that"</li> }
                            .into_any();
                    }

                    listed
                        .into_iter()
                        .enumerate()
                        .map(|(index, choice)| {
                            let taken = choice.clone();

                            view! {
                                <li
                                    role="option"
                                    aria-selected=(Some(&choice.value) == chosen.as_ref()).to_string()
                                    aria-disabled=choice.disabled.to_string()
                                    class=(style::at, move || at.get() == Some(index))
                                    on:pointerenter=move |_| highlight(open, index)
                                    // Keeps the focus where it is so the list
                                    // stays open for the click to land.
                                    on:mousedown=|event| event.prevent_default()
                                    on:click=move |_| on_take.run(taken.clone())
                                >
                                    {choice
                                        .dot
                                        .map(|(dot, state)| {
                                            view! { <span class=dot.class() title=state></span> }
                                        })}
                                    {match choice.detail {
                                        Some(detail) => view! {
                                            <span class=style::lines>
                                                <span class=style::name>{choice.label}</span>
                                                <span class=style::detail>{detail}</span>
                                            </span>
                                        }
                                            .into_any(),
                                        None => view! { <span class=style::name>{choice.label}</span> }.into_any(),
                                    }}
                                    {choice.note.map(|note| view! { <span class=style::note>{note}</span> })}
                                </li>
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
            </ul>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_opens_upwards_only_with_more_room_above() {
        assert_eq!(placement(500.0, 400.0, 3, false), (false, 400.0));
        assert_eq!(placement(500.0, 100.0, 3, false), (true, 500.0));
        assert_eq!(placement(50.0, 60.0, 3, false), (false, ROW * 2.0));
    }
}
