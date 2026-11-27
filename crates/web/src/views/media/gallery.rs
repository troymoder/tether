use leptos::prelude::*;
use leptos::{ev, html};
use stylance::classes;

use super::thumbnails::Thumbnails;
use super::tiles::Tiles;
use super::viewer::{Viewed, Viewer};
use super::{Item, Upload, style};
use crate::app::{Global, Picked};
use crate::device::Helper;
use crate::link;
use crate::ui;
use crate::ui::action::use_action;
use crate::ui::load::use_load;
use crate::ui::loadable::Loadable;
use crate::ui::menu::use_menu;
use crate::ui::pager::{Pager, Pages};
use crate::ui::pane::Pane;
use crate::ui::pick::{PickedBar, Picking, selection_menu};
use crate::util::counted;

const PAGE: usize = 60;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Gallery(
    helper: Helper,
    changed: Trigger,
    add: Callback<web_sys::File>,
) -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let device = expect_context::<Picked>().device;
    let helper = StoredValue::new_local(helper);
    let picking = Picking::new();
    let total = use_load(
        move || changed.track(),
        move |()| {
            let helper = helper.get_value();

            async move { helper.count().await.map(|count| count as usize) }
        },
    );
    // The gallery is paged because a phone holds thousands of items and is
    // asked for a thumbnail of each one shown.
    let pages = Memo::new(move |_| total.get().ready().map(|all| Pages { all, each: PAGE }));
    let asked = RwSignal::new(0usize);
    // The shown page is clamped to the last one, because deleting items can
    // leave the asked page past the end.
    let page = Memo::new(move |_| {
        let asked = asked.get();

        pages.get().map_or(asked, |pages| pages.clamped(asked))
    });
    let items = use_load(
        move || {
            changed.track();
            page.get()
        },
        move |at: usize| {
            let helper = helper.get_value();

            async move {
                let items = helper.page(at * PAGE, PAGE).await?;
                let items: Vec<Item> = items
                    .into_iter()
                    .map(|medium| Item {
                        path: medium.path,
                        size: medium.size,
                        changed: medium.changed.to_string(),
                    })
                    .collect();

                Ok(items)
            }
        },
    );
    let thumbnails = Thumbnails::start(helper, items);
    let top = NodeRef::<html::Div>::new();
    let viewing = RwSignal::new(None::<String>);
    // The viewed item is looked up on the loaded page, so deleting it closes
    // the viewer.
    let viewed = Memo::new(move |_| {
        let path = viewing.get()?;

        items.with(|items| match items {
            Loadable::Ready(items) => Viewed::among(items, &path),
            Loadable::Loading | Loadable::Failed(_) => None,
        })
    });
    let picked = RwSignal::new(Vec::<String>::new());
    let shown = Signal::derive(move || {
        items.with(|items| match items {
            Loadable::Ready(items) => items
                .iter()
                .map(|item| item.path.clone())
                .collect::<Vec<String>>(),
            Loadable::Loading | Loadable::Failed(_) => Vec::new(),
        })
    });
    let menu = use_menu::<Item>();

    let turn = move |to: usize| {
        asked.set(to);
        // The viewed item belongs to the page being left.
        viewing.set(None);

        if let Some(top) = top.get_untracked() {
            top.set_scroll_top(0.0);
        }
    };

    let (_, save) = use_action(Some("media"), move |paths: Vec<String>| {
        let device = device.get_untracked();

        link.say(format!("Fetching {}…", counted(paths.len())));

        async move {
            for path in &paths {
                let file = device.read(path).await?;

                link::save(path.rsplit('/').next().unwrap_or(path), &file);
            }

            Ok("Saved them to this computer".to_owned())
        }
    });
    let (_, remove) = use_action(Some("media"), move |paths: Vec<String>| {
        let device = device.get_untracked();

        async move {
            let mut said = Ok(format!("Deleted {}", counted(paths.len())));

            for path in &paths {
                if let Err(error) = device.delete(path).await {
                    said = Err(error);
                    break;
                }

                picked.try_update(|picked| picked.retain(|other| other != path));
                viewing.try_update(|viewing| {
                    if viewing.as_ref() == Some(path) {
                        *viewing = None;
                    }
                });
            }

            changed.notify();

            said
        }
    });
    let delete = Callback::new(move |paths: Vec<String>| {
        let deleted = counted(paths.len());

        ui.confirm_then(
            format!("Delete {deleted}?"),
            "They are deleted from the device.",
            "Delete",
            async move {
                remove.try_run(paths);
            },
        );
    });

    view! {
        <Pane
            title="Media"
            under=move || {
                view! {
                    <div class=style::row>
                        <Upload add />
                        <Pager page pages on_turn=turn />
                    </div>
                }
            }
            on:pointerdown=move |event: ev::PointerEvent| {
                if viewed.with_untracked(Option::is_none) {
                    picking.start(&event, style::swept, picked);
                }
            }
        >
            <div class=style::under>
                <div class=classes!(ui::style::pane_body, style::body) node_ref=top>
                    <Tiles total items pages page viewing picked menu thumbnails picking />
                    <For each=move || viewed.get() key=Viewed::key let:viewed>
                        <Viewer viewed viewing picked scroller=top />
                    </For>
                </div>
                <PickedBar
                    picked
                    shown
                    all="Pick the page"
                    on_download=move |()| save.run(picked.get_untracked())
                    on_delete=move |()| delete.run(picked.get_untracked())
                />
            </div>
            {menu
                .view(
                    false,
                    move |item: &Item| {
                        Some(
                            selection_menu(
                                picked,
                                item.path.clone(),
                                item.name().to_owned(),
                                save,
                                Some(delete),
                            ),
                        )
                    },
                )}
        </Pane>
    }
}
