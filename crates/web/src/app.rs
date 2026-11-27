use leptos::context::Provider;
use leptos::ev;
use leptos::html;
use leptos::prelude::*;

use crate::device::Views;
use crate::link::{Link, Linked};
use crate::ui;
use crate::views::access::{Access, Settings};
use crate::views::apps::Apps;
use crate::views::camera::Camera;
use crate::views::clipboard::Clipboard;
use crate::views::crashes::Crashes;
use crate::views::data::Database;
use crate::views::devices::Devices;
use crate::views::events::Events;
use crate::views::files::Files;
use crate::views::location::Location;
use crate::views::logs::Logs;
use crate::views::media::Media;
use crate::views::network::Network;
use crate::views::preferences::Preferences;
use crate::views::registry::Registry;
use crate::views::screen::{Absent, Screen};
use crate::views::shell::Shell;

mod dialogs;
mod dock;
mod drop;
mod favicon;
mod gate;
pub(crate) mod icons;
mod page;
mod panel;
mod route;
mod scope;
pub(crate) mod storage;
mod theme;
mod toast;
mod top;

pub(crate) use icons::Icon;
pub(crate) use page::{Adding, Panel, Ui, Wanted};
pub(crate) use route::{Page, Router};
pub(crate) use scope::Picked;
pub(crate) use storage::{Read, forget, keep, kept, read};
pub(crate) use theme::Theme;

use dialogs::{Add, Lost};
use dock::{Dock, Menu};
use panel::{Edge, Layout};
use scope::{Needs, NoDevice};
use toast::Toast;
use top::Top;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "app.module.scss"
);

#[allow(unreachable_pub)]
#[component]
pub(crate) fn App() -> impl IntoView {
    // The page waits for these before it draws, so that it starts as it was
    // left and does not move from the defaults.
    let settings = LocalResource::new(|| {
        read(
            [
                theme::THEME_KEY,
                panel::WIDTH_KEY,
                panel::PANEL_KEY,
                dock::FOLD_KEY,
                dock::SETUP_KEY,
            ]
            .map(str::to_owned),
        )
    });

    // No Suspense here. Every resource the page reads later would register
    // with it, and while any of them loads the whole page would blank.
    move || settings.get().map(|settings| view! { <Page settings /> })
}

#[derive(Clone, Copy)]
pub(crate) struct Global {
    pub link: Link,
    pub router: Router,
    pub ui: Ui,
}

#[component]
fn Page(settings: Read) -> impl IntoView {
    let ui = Ui::new(&settings);
    let stored = route::Route::restored();
    let link = Link::start(
        Callback::new(move |text: String| ui.say(text)),
        Signal::derive(move || stored.with(|route| route.device.clone())),
    );
    let router = Router::start(stored, ui, link);
    let global = Global { link, router, ui };

    view! {
        <Provider value=global>
            <Sides settings />
        </Provider>
    }
}

#[component]
fn Sides(settings: Read) -> impl IntoView {
    let Global { link, router, ui } = expect_context::<Global>();
    let drops = ui.drops;
    let layout = Layout::new(&settings);
    let menu = Menu::new(layout, &settings);
    let picked = scope::use_picked();

    drop::listen();
    favicon::follow();

    // An open dialog handles Escape itself.
    let _ = window_event_listener(ev::keydown, move |event| {
        let popup = document()
            .query_selector("dialog[open]")
            .ok()
            .flatten()
            .is_some();

        if event.key() != "Escape" || event.default_prevented() || popup {
            return;
        }

        if menu.is_over() {
            menu.over.set(false);
        } else if layout.over.get_untracked() && ui.panel.get_untracked() == Panel::Open {
            ui.panel.set(Panel::Auto);
        }
    });

    let sides = NodeRef::<html::Div>::new();
    // The discriminant alone, so that the view stays while it only moves
    // within itself, as Files does from folder to folder.
    let view = Memo::new(move |_| router.on(std::mem::discriminant));
    let question = move || {
        ui.confirm.with(|confirm| {
            confirm
                .as_ref()
                .map(|confirm| (confirm.title.clone(), confirm.body.clone(), confirm.action))
        })
    };

    view! {
        // Each class is a separate attribute because panel.rs toggles
        // `sizing` by hand during a drag, which a whole class string would
        // overwrite.
        <div
            node_ref=sides
            style=move || layout.sizes()
            class=style::sides
            class=(style::dropping, move || drops.dragging.with(Option::is_some))
            class=(style::refused, move || drops.dragging.get() == Some(false))
            class=(style::to_files, move || router.on(|page| matches!(page, Page::Files { .. })))
            class=(style::to_media, move || router.on(|page| matches!(page, Page::Media)))
            class=(style::full, move || layout.full.get())
        >
            <div class=style::views class=(style::away, move || !layout.shown.get())>
                <Top full=layout.full menu_over=menu.over />
                <div class=style::view>
                    <Dock menu />
                    <main
                        class=style::main
                        class=(style::bleed, move || router.on(|page| matches!(page, Page::Location)))
                        class=(ui::style::waiting, move || !link.loaded.get())
                        aria-busy=move || (!link.loaded.get()).to_string()
                    >
                        {move || {
                            // The view is built again once tetherd sends its
                            // state, so that it starts from it.
                            link.loaded.track();
                            // The memo is read, because one that is only
                            // tracked is never worked out and so never
                            // changes.
                            view.get();

                            let about = move |view: Views, title: &'static str, shown: fn() -> AnyView| {
                                view! {
                                    <For each=move || picked.get() key=|picked| picked.id.clone() let:picked>
                                        <Provider value=picked>
                                            <Needs view title>
                                                {shown()}
                                            </Needs>
                                        </Provider>
                                    </For>
                                    <Show when=move || picked.with(Option::is_none)>
                                        <NoDevice title />
                                    </Show>
                                }
                                    .into_any()
                            };

                            match router.page_untracked() {
                                Page::Devices { .. } => view! { <Devices /> }.into_any(),
                                Page::Registry { .. } => view! { <Registry /> }.into_any(),
                                Page::Events => view! { <Events /> }.into_any(),
                                Page::Access => view! { <Access /> }.into_any(),
                                Page::Settings => view! { <Settings /> }.into_any(),
                                Page::Network => view! { <Network /> }.into_any(),
                                Page::Apps => about(Views::APPS, "Apps", || view! { <Apps /> }.into_any()),
                                Page::Preferences => {
                                    about(Views::PREFERENCES, "Preferences", || view! { <Preferences /> }.into_any())
                                }
                                Page::Location => {
                                    about(Views::LOCATION, "Location", || view! { <Location /> }.into_any())
                                }
                                Page::Files { .. } => about(Views::FILES, "Files", || view! { <Files /> }.into_any()),
                                Page::Media => about(Views::MEDIA, "Media", || view! { <Media /> }.into_any()),
                                Page::Camera => about(Views::CAMERA, "Camera", || view! { <Camera /> }.into_any()),
                                Page::Database { .. } => {
                                    about(Views::DATABASE, "Database", || view! { <Database /> }.into_any())
                                }
                                Page::Clipboard => {
                                    about(Views::CLIPBOARD, "Clipboard", || view! { <Clipboard /> }.into_any())
                                }
                                Page::Crashes => {
                                    about(Views::CRASHES, "Crashes", || view! { <Crashes /> }.into_any())
                                }
                                Page::Logs => about(Views::LOGS, "Logs", || view! { <Logs /> }.into_any()),
                                Page::Shell => about(Views::SHELL, "Shell", || view! { <Shell /> }.into_any()),
                            }
                        }}
                    </main>
                </div>
            </div>
            <Edge layout sides />
            <button
                type="button"
                class=style::scrim
                class=(style::over, move || layout.shown.get() && layout.over.get())
                title="Back to the device"
                aria-label="Back to the device"
                on:click=move |_| ui.panel.set(Panel::Auto)
            ></button>
            <For each=move || picked.get() key=|picked| picked.id.clone() let:picked>
                <Provider value=picked>
                    <Screen />
                </Provider>
            </For>
            <Show when=move || picked.with(Option::is_none)>
                <Absent />
            </Show>
        </div>

        <Show when=move || ui.adding.with(Option::is_some)>
            <Add />
        </Show>

        {move || {
            question().map(|(title, body, action)| view! { <dialogs::Sure title body action /> })
        }}

        <Show when=move || link.linked.with(Linked::lost)>
            <Lost />
        </Show>

        <gate::Gate />

        <Toast />
    }
}
