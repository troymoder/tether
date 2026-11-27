//! A view makes one menu for all its rows with [`use_menu`], opens it from a
//! row's events, and renders it once. Each row turns into what clicking it
//! does while hovered or focused, in the same grid cell, so that the menu
//! never changes width.

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use stylance::classes;
use wasm_bindgen::JsCast;
use web_sys::Node;

use crate::app::Icon;
use crate::app::icons;
use crate::ui::floating::{Anchor, close_on_scroll, use_floating};

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "menu.module.scss"
);

#[derive(Clone)]
pub(crate) struct Row {
    pub what: String,
    pub icon: Option<&'static str>,
    pub does_icon: Option<&'static str>,
    pub place: Option<String>,
    pub ticked: bool,
    /// Shown in place of the row while it is hovered or focused. None for a
    /// row that only says how something is, as "Booting…" does.
    pub does: Option<String>,
    pub undoes: bool,
    pub on: Callback<()>,
}

#[derive(Clone)]
pub(crate) enum Item {
    Label(&'static str),
    Row(Row),
    Rule,
    Action(&'static str, Callback<()>),
    Keyed {
        text: &'static str,
        keys: Option<&'static str>,
        red: bool,
        on: Callback<()>,
    },
}

pub(crate) struct MenuOf<T: 'static>(RwSignal<Option<(Anchor, T)>>);

impl<T> Clone for MenuOf<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for MenuOf<T> {}

pub(crate) fn use_menu<T>() -> MenuOf<T>
where
    T: Clone + Send + Sync + 'static,
{
    MenuOf(RwSignal::new(None))
}

impl<T> MenuOf<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub(crate) fn at_pointer(self, event: &ev::MouseEvent, of: T) {
        let at = Anchor::Point {
            x: event.client_x(),
            y: event.client_y(),
        };

        event.prevent_default();
        self.0.set(Some((at, of)));
    }

    pub(crate) fn under(self, event: &ev::MouseEvent, of: T) {
        let button = event
            .current_target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok());

        if let Some(button) = button {
            let under = button.get_bounding_client_rect();
            let at = Anchor::Point {
                x: under.left(),
                y: under.bottom(),
            };

            self.0.set(Some((at, of)));
        }
    }

    /// `rows` gives the heading and the rows for what the menu opened on, as
    /// that was when it opened, or None to open nothing.
    pub(crate) fn view(
        self,
        raised: bool,
        rows: impl Fn(&T) -> Option<(String, Vec<Item>)> + Send + Sync + 'static,
    ) -> impl IntoView {
        move || {
            let (at, of) = self.0.get()?;
            let (heading, items) = untrack(|| rows(&of))?;

            Some(view! { <Menu at heading items close=move |()| self.0.set(None) raised /> })
        }
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Menu(
    at: Anchor,
    heading: String,
    items: Vec<Item>,
    #[prop(into)] close: Callback<()>,
    #[prop(optional)] raised: bool,
) -> impl IntoView {
    let boxed = NodeRef::<html::Div>::new();
    let floating = use_floating(boxed);

    floating.place(
        move || Some(at),
        |boxed| {
            // A hidden box can't take focus, and its place only lands once
            // the signal has run, so the focus waits a frame.
            request_animation_frame(move || {
                let _ = boxed.focus();
            });
        },
    );

    close_on_scroll(|_| true, move || close.run(()));

    // A click on something that takes no focus moves none, so focusout alone
    // misses it.
    let away = window_event_listener(ev::pointerdown, move |event| {
        let Some(boxed) = boxed.try_get_untracked().flatten() else {
            return;
        };

        let on = event.target().and_then(|on| on.dyn_into::<Node>().ok());

        if !on.is_some_and(|on| boxed.contains(Some(&on))) {
            close.run(());
        }
    });

    on_cleanup(move || away.remove());

    let item = move |item: Item| match item {
        Item::Label(text) => view! { <span class=style::label>{text}</span> }.into_any(),
        Item::Rule => view! { <span class=style::rule role="separator"></span> }.into_any(),
        Item::Action(text, on) => view! {
            <button
                type="button"
                role="menuitem"
                class=style::action
                on:click=move |_| {
                    close.run(());
                    on.run(());
                }
            >
                {text}
            </button>
        }
        .into_any(),
        Item::Keyed {
            text,
            keys,
            red,
            on,
        } => view! {
            <button
                type="button"
                role="menuitem"
                class=classes!(style::action, style::keyed, red.then_some(style::red))
                on:click=move |_| {
                    close.run(());
                    on.run(());
                }
            >
                <span>{text}</span>
                {keys.map(|keys| {
                    view! { <span class=style::keys aria-hidden="true">{keys}</span> }
                })}
            </button>
        }
        .into_any(),
        Item::Row(row) => {
            let in_use = if row.ticked { ", in use" } else { "" };
            let place = row
                .place
                .as_ref()
                .map(|place| format!(", {place}"))
                .unwrap_or_default();
            let does = row.does.clone();
            let label = match &does {
                Some(does) => format!("{}{place}{in_use}. {does}", row.what),
                None => format!("{}{place}{in_use}", row.what),
            };
            let on = row.on;
            let dead = does.is_none();

            view! {
                <button
                    type="button"
                    role="menuitem"
                    class=classes!(
                        style::row,
                        row.undoes.then_some(style::undoes),
                        dead.then_some(style::dead)
                    )
                    aria-label=label
                    disabled=dead
                    on:click=move |_| {
                        close.run(());
                        on.run(());
                    }
                >
                    {row
                        .ticked
                        .then(|| {
                            view! {
                                <svg class=style::mark viewBox="0 0 24 24" aria-hidden="true" inner_html=icons::CHECK></svg>
                            }
                        })}
                    {(row.ticked && row.undoes)
                        .then(|| {
                            view! {
                                <svg class=style::cross viewBox="0 0 24 24" aria-hidden="true" inner_html=icons::CLOSE></svg>
                            }
                        })}
                    <span class=style::is aria-hidden="true">
                        <span class=style::named>
                            {row.icon.map(|paths| view! { <Icon paths /> })}
                            <span>{row.what}</span>
                        </span>
                        <span class=style::place>{row.place}</span>
                    </span>
                    <span class=classes!(style::does, style::named) aria-hidden="true">
                        {row.does_icon.map(|paths| view! { <Icon paths /> })}
                        <span>{does}</span>
                    </span>
                </button>
            }
            .into_any()
        }
    };

    let named = heading.clone();

    view! {
        <div
            node_ref=boxed
            class=classes!(style::menu, raised.then_some(style::raised))
            role="menu"
            aria-label=named
            popover="manual"
            tabindex="-1"
            style=move || floating.style()
            on:focusout=move |event| {
                let Some(boxed) = boxed.try_get_untracked().flatten() else {
                    return;
                };

                let to = event.related_target().and_then(|to| to.dyn_into::<Node>().ok());

                if !to.is_some_and(|to| boxed.contains(Some(&to))) {
                    close.run(());
                }
            }
            on:keydown=move |event: ev::KeyboardEvent| {
                if event.key() == "Escape" {
                    event.prevent_default();
                    event.stop_propagation();
                    close.run(());
                }
            }
        >
            <span class=style::heading>{heading}</span>
            {items.into_iter().map(item).collect_view()}
        </div>
    }
}
