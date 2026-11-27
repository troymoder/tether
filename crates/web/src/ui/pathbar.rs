use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use leptos::prelude::*;
use leptos::{ev, html};
use wasm_bindgen::JsCast;

use crate::ui::loadable::Loadable;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "pathbar.module.scss"
);

/// Each folder's name and path.
type Inside = Vec<(String, String)>;

pub(crate) type Folders =
    Rc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<Inside, String>>>>>;

/// Each part's name and path, from the root.
fn parts(path: &str) -> Vec<(String, String)> {
    let mut parts = vec![("/".to_owned(), "/".to_owned())];
    let mut so_far = String::new();

    for part in path.split('/').filter(|part| !part.is_empty()) {
        so_far = format!("{so_far}/{part}");
        parts.push((part.to_owned(), so_far.clone()));
    }

    parts
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn PathBar(
    #[prop(into)] path: Signal<String>,
    folders: Folders,
    /// Called with the path of a part, or of a folder picked from a list.
    #[prop(into)]
    on_go: Callback<String>,
    find: RwSignal<String>,
    /// Called with a path entered in the box or in the editable path.
    #[prop(into)]
    on_path: Callback<String>,
) -> impl IntoView {
    let open = RwSignal::new(None::<(usize, Loadable<Inside>)>);
    // The list's position in the window, under its › wherever the parts have
    // scrolled to.
    let under = RwSignal::new((0.0, 0.0));
    let typing = RwSignal::new(false);
    let shown = NodeRef::<html::Span>::new();
    let folders = StoredValue::new_local(folders);
    let pick = Callback::new(move |to: String| {
        open.set(None);
        on_go.run(to);
    });

    // A long path stays scrolled to its end, the current folder.
    Effect::new(move |_| {
        path.track();

        if let Some(shown) = shown.get() {
            request_animation_frame(move || shown.set_scroll_left(f64::from(shown.scroll_width())));
        }
    });

    let list = move |at: usize, of: String, from: Option<web_sys::Element>| {
        if open.with_untracked(|open| matches!(open, Some((was, _)) if *was == at)) {
            open.set(None);
            return;
        }

        if let Some(from) = from {
            let place = from.get_bounding_client_rect();

            under.set((place.left(), place.bottom() + 4.0));
        }

        open.set(Some((at, Loadable::Loading)));

        let Some(listing) = folders.try_with_value(|folders| folders(of)) else {
            return;
        };

        leptos::task::spawn_local(async move {
            let found = listing.await;

            // Dropped when another list was opened meanwhile.
            open.try_update(|open| {
                if let Some((now, kids)) = open
                    && *now == at
                {
                    *kids = found.into();
                }
            });
        });
    };

    let parts_view = move || {
        parts(&path.get())
            .into_iter()
            .enumerate()
            .map(|(at, (name, to))| {
                let going = to.clone();
                let listing = to.clone();

                view! {
                    <span class=style::part>
                        <button
                            type="button"
                            class=style::name
                            title=format!("Go to {name}")
                            on:click=move |_| on_go.run(going.clone())
                        >
                            {name.clone()}
                        </button>
                        <button
                            type="button"
                            class=style::more
                            aria-label=format!("Folders in {name}")
                            aria-expanded=move || open.with(|open| matches!(open, Some((was, _)) if *was == at)).to_string()
                            on:click=move |event: ev::MouseEvent| {
                                let from = event
                                    .target()
                                    .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                                    .and_then(|target| target.closest("button").ok().flatten());

                                list(at, listing.clone(), from);
                            }
                        >
                            <svg aria-hidden="true" viewBox="0 0 24 24">
                                <path d="M9 6l6 6-6 6" />
                            </svg>
                        </button>
                        {move || {
                            let kids = open.with(|open| match open {
                                Some((was, kids)) if *was == at => Some(kids.clone()),
                                _ => None,
                            })?;

                            Some(view! { <FolderList kids under on_pick=pick /> })
                        }}
                    </span>
                }
            })
            .collect_view()
    };

    view! {
        <div class=style::bar class=(style::typing, move || typing.get())>
            <Show when=move || open.with(Option::is_some)>
                <button
                    type="button"
                    class=style::away
                    aria-label="Close"
                    on:click=move |_| open.set(None)
                ></button>
            </Show>
            <Show
                when=move || typing.get()
                fallback=move || {
                    view! {
                        <span
                            node_ref=shown
                            class=style::parts
                            title="Double-click to type a path"
                            on:dblclick=move |_| {
                                open.set(None);
                                typing.set(true);
                            }
                        >
                            {parts_view}
                        </span>
                        <span class=style::rule aria-hidden="true"></span>
                        <input
                            class=style::find
                            aria-label="Find here, or go to a path"
                            placeholder="Find here, or paste a path"
                            autocomplete="off"
                            spellcheck="false"
                            bind:value=find
                            on:keydown=move |event: ev::KeyboardEvent| match event.key().as_str() {
                                "Enter" => {
                                    event.prevent_default();
                                    on_path.run(find.get_untracked());
                                }
                                // Clears the filter and stops the key so
                                // nothing around the bar closes.
                                "Escape" if !find.with_untracked(String::is_empty) => {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    find.set(String::new());
                                }
                                _ => {}
                            }
                        />
                    }
                }
            >
                <PathField typing path=path.get_untracked() on_path />
            </Show>
        </div>
    }
}

#[component]
fn FolderList(
    kids: Loadable<Inside>,
    under: RwSignal<(f64, f64)>,
    on_pick: Callback<String>,
) -> impl IntoView {
    view! {
        <span
            role="menu"
            class=style::menu
            style=move || {
                let (left, top) = under.get();

                format!("left: {left}px; top: {top}px")
            }
        >
            {match kids {
                Loadable::Loading => view! { <span class=style::said>"…"</span> }.into_any(),
                Loadable::Failed(why) => view! { <span class=style::said>{why}</span> }.into_any(),
                Loadable::Ready(kids) if kids.is_empty() => {
                    view! { <span class=style::said>"No folders"</span> }.into_any()
                }
                Loadable::Ready(kids) => kids
                    .into_iter()
                    .map(|(name, to)| {
                        view! {
                            <button
                                type="button"
                                role="menuitem"
                                on:click=move |_| on_pick.run(to.clone())
                            >
                                {name}
                            </button>
                        }
                    })
                    .collect_view()
                    .into_any(),
            }}
        </span>
    }
}

#[component]
fn PathField(typing: RwSignal<bool>, path: String, on_path: Callback<String>) -> impl IntoView {
    let draft = RwSignal::new(path);
    let typed = NodeRef::<html::Input>::new();

    Effect::new(move |_| {
        if let Some(field) = typed.get() {
            let _ = field.focus();

            field.select();
        }
    });

    view! {
        <input
            node_ref=typed
            class=style::whole
            aria-label="Path"
            autocomplete="off"
            spellcheck="false"
            bind:value=draft
            on:keydown=move |event: ev::KeyboardEvent| match event.key().as_str() {
                "Enter" => {
                    let entered = draft.get_untracked();

                    event.prevent_default();
                    typing.set(false);
                    on_path.run(entered);
                }
                "Escape" => {
                    event.prevent_default();
                    event.stop_propagation();
                    typing.set(false);
                }
                _ => {}
            }
            on:blur=move |_| typing.set(false)
        />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_its_parts_from_the_root() {
        assert_eq!(parts("/"), [("/".to_owned(), "/".to_owned())]);
        assert_eq!(
            parts("/sdcard/Download/"),
            [
                ("/".to_owned(), "/".to_owned()),
                ("sdcard".to_owned(), "/sdcard".to_owned()),
                ("Download".to_owned(), "/sdcard/Download".to_owned()),
            ]
        );
    }
}
