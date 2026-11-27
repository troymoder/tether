use leptos::ev;
use leptos::prelude::*;
use protocol::{App, Platform, Request, Topic};
use stylance::classes;

// Renamed because `Page` in this module is the page of one app.
use crate::app::{Global, Icon, Page as Open, icons};
use crate::link::{self, ServerStoreFields, unimplemented};
use crate::ui;
use crate::ui::action::use_action_many;
use crate::ui::button::Button;
use crate::ui::file_pick::{FilePick, FilePicker};
use crate::ui::logo::platform_logo;
use crate::ui::pane::Pane;
use crate::ui::search::{Empty, SearchBox, matches};
use crate::ui::segmented::Segmented;
use crate::views::apps::icon::{AppIcon, use_registry_icon};
use crate::views::apps::{called, use_app, versioned};

mod page;

use page::Page;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "registry.module.scss"
);

#[derive(Clone, Copy, PartialEq)]
enum Shown {
    All,
    Android,
    Ios,
}

/// The registry only accepts APKs, so this is always Android.
fn platforms(_app: &App) -> &'static [Platform] {
    &[Platform::Android]
}

// `use_action_many` keeps any file from being dropped while another uploads.
pub(crate) fn use_upload() -> Callback<web_sys::File> {
    let Global { link, router, .. } = expect_context::<Global>();
    let (_, upload) = use_action_many(None, move |file: web_sys::File| async move {
        let name = file.name();

        if name.ends_with(".aab") || name.ends_with(".ipa") {
            return Err(unimplemented(&format!("upload {name}")));
        }

        if !name.ends_with(".apk") {
            return Err(format!("not an app: {name}"));
        }

        link.say(format!("Adding {name}…"));

        let apk = link::read(&file)
            .await
            .ok_or_else(|| format!("read failed: {name}"))?;
        // tetherd answers with the package name it read from the APK.
        let id = link
            .call_large(Request::AddApp { apk, device: None })
            .await?;

        if !matches!(router.page_untracked(), Open::Registry { .. }) {
            router.go(Open::registry());
        }

        Ok(format!("Added {id}"))
    });

    upload
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Registry() -> impl IntoView {
    let Global { link, router, .. } = expect_context::<Global>();

    link.follow(|| Some(Topic::Apps));

    // Removing the app from the registry leaves its page.
    let open = Memo::new(move |_| {
        let Open::Registry { app: Some(id) } = router.page() else {
            return None;
        };

        link.server
            .apps()
            .with(|apps| apps.iter().any(|app| app.id == id))
            .then_some(id)
    });

    move || {
        let app = open.get().and_then(|id| {
            // Untracked because the page follows the app by itself once it
            // is open.
            untrack(|| link.server.apps().at_key(id).try_get())
        });

        match app {
            Some(app) => view! { <Page app /> }.into_any(),
            None => view! { <List /> }.into_any(),
        }
    }
}

#[allow(unreachable_pub)]
#[component]
fn List() -> impl IntoView {
    let link = expect_context::<Global>().link;
    let sought = RwSignal::new(String::new());
    let shown = RwSignal::new(Shown::All);
    let builds = FilePicker::new();
    let upload = use_upload();
    let listed = Memo::new(move |_| {
        let sought = sought.get();
        let shown = shown.get();
        let mut apps: Vec<App> = link.server.apps().with(|apps| {
            apps.iter()
                .filter(|app| match shown {
                    Shown::All => true,
                    Shown::Android => platforms(app).contains(&Platform::Android),
                    Shown::Ios => platforms(app).contains(&Platform::Ios),
                })
                .filter(|app| matches(&sought, &[&called(app), &app.id]))
                .cloned()
                .collect()
        });

        apps.sort_by_cached_key(|app| called(app).to_lowercase());

        apps
    });

    view! {
        <Pane
            title="Registry"
            tools=move || {
                view! {
                    <span class=style::tools>
                        <SearchBox
                            sought
                            label="Show only the apps with this in their name or id"
                            placeholder="Search apps"
                        />
                        <Button
                            class=style::upload
                            attr:title="Or drop .apk, .aab or .ipa files on the page"
                            on:click=move |_| builds.open()
                        >
                            <Icon paths=icons::UPLOAD />
                            <span>"Upload builds"</span>
                        </Button>
                    </span>
                }
            }
        >
            <div class=ui::style::pane_body>
                <div class=style::under>
                    <Segmented
                        label="Which platform is listed"
                        choices=vec![
                            (Shown::All, "All"),
                            (Shown::Android, "Android"),
                            (Shown::Ios, "iOS"),
                        ]
                        chosen=shown
                        on_choose=move |choice| shown.set(choice)
                    />
                    <code class=style::cli>"tetherd app upload <file>"</code>
                </div>
                <FilePick
                    picker=builds
                    multiple=true
                    accept=".apk,.aab,.ipa"
                    label="Builds for the registry"
                    on_file=upload
                />
                <Empty
                    all=Signal::derive(move || link.server.apps().with(Vec::len))
                    shown=Signal::derive(move || listed.with(Vec::len))
                    none="No builds in this registry yet."
                />
                <div class=style::table>
                    <div class=style::apps>
                        <div class=classes!(style::app, style::names_row) aria-hidden="true">
                            <span></span>
                            <span>"App"</span>
                            <span>"For"</span>
                            <span>"Latest"</span>
                            <span>"Builds"</span>
                            <span>"Auto-update"</span>
                        </div>
                        <For each=move || listed.get() key=|app| app.id.clone() let:app>
                            <ListRow app />
                        </For>
                    </div>
                </div>
                // The page's own drop listener in app/drop.rs takes files
                // dropped here.
                <button type="button" class=style::drop on:click=move |_| builds.open()>
                    "Drop .apk, .aab or .ipa files here"
                </button>
            </div>
        </Pane>
    }
}

#[allow(unreachable_pub)]
#[component]
fn ListRow(app: App) -> impl IntoView {
    let Global { link, router, .. } = expect_context::<Global>();
    let logos = platforms(&app)
        .iter()
        .map(|platform| platform_logo(*platform))
        .collect_view();
    let id = app.id.clone();
    let app = use_app(app);
    let drawn = use_registry_icon(move || Some(app.get()));
    let latest = move || app.with(|app| app.latest().map_or_else(|| "—".to_owned(), versioned));
    let auto = move || {
        app.with(|app| match app.auto.as_slice() {
            [] => "—".to_owned(),
            [one] => link.name(one),
            many => format!("{} devices", many.len()),
        })
    };
    let open = {
        let id = id.clone();

        move || {
            router.go(Open::Registry {
                app: Some(id.clone()),
            })
        }
    };
    let pressed = open.clone();

    view! {
        <div
            class=style::app
            role="button"
            tabindex="0"
            on:click=move |_| open()
            on:keydown=move |event: ev::KeyboardEvent| {
                if event.key() == "Enter" || event.key() == " " {
                    event.prevent_default();
                    pressed();
                }
            }
        >
            <AppIcon id=id.clone() drawn />
            <span class=style::names>
                <strong>{move || app.with(called)}</strong>
                <code>{id}</code>
            </span>
            <span class=style::logos>{logos}</span>
            <code class=style::latest_build>{latest}</code>
            <span class=style::muted>{move || app.with(|app| app.builds)}</span>
            <span class=classes!(style::muted, style::cut)>{auto}</span>
        </div>
    }
}
