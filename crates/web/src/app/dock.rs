use leptos::prelude::*;
use stylance::classes;

use super::icons::{self, Icon};
use super::panel::{Layout, PANEL};
use super::storage::Read;
use super::theme::Theme;
use super::{Global, Page, style};
use crate::link::ServerStoreFields;
use crate::ui;
use crate::ui::kept::{KeptHere, kept_started};
use crate::ui::segmented::Segmented;

pub(super) const FOLD_KEY: &str = "menu-folded";
pub(super) const SETUP_KEY: &str = "menu-setup-down";

#[derive(Clone, Copy, PartialEq)]
enum Fold {
    Names,
    Icons,
    /// The names over the view, on a panel too narrow for them beside it.
    Over,
}

impl Fold {
    fn of(small: bool, over: bool, folded: bool) -> Self {
        match (small, over, folded) {
            (true, true, _) => Fold::Over,
            (true, false, _) | (false, _, true) => Fold::Icons,
            (false, _, false) => Fold::Names,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Menu {
    folded: KeptHere<bool>,
    pub(super) over: RwSignal<bool>,
    small: Memo<bool>,
    fold: Memo<Fold>,
    setup_down: KeptHere<bool>,
}

impl Menu {
    pub(super) fn new(layout: Layout, settings: &Read) -> Self {
        let folded = kept_started(settings, FOLD_KEY, false);
        let over = RwSignal::new(false);
        let small = Memo::new(move |_| layout.shown.get() && layout.wide.get() < PANEL);
        let fold = Memo::new(move |_| Fold::of(small.get(), over.get(), folded.get()));
        let setup_down = kept_started(settings, SETUP_KEY, false);

        Menu {
            folded,
            over,
            small,
            fold,
            setup_down,
        }
    }

    pub(super) fn is_over(self) -> bool {
        self.fold.get_untracked() == Fold::Over
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Dock(menu: Menu) -> impl IntoView {
    let Menu {
        over,
        fold,
        setup_down,
        ..
    } = menu;

    view! {
        <nav
            aria-label="Views"
            class=style::dock
            class=(style::collapsed, move || fold.get() == Fold::Icons)
            class=(style::over, move || fold.get() == Fold::Over)
            class=(style::setup_down, move || setup_down.get())
        >
            <div class=style::part>
                <Entry page=Page::devices() title="Devices" paths=icons::DEVICES over />
                <Entry page=Page::registry() title="Registry" paths=icons::LAYERS over />
            </div>
            <div class=style::part>
                <span>"Device"</span>
                <Entry page=Page::Apps title="Apps" paths=icons::APPS over />
                <Entry page=Page::Logs title="Logs" paths=icons::LOGS over />
                <Entry page=Page::Crashes title="Crashes" paths=icons::CRASH over />
                <Entry page=Page::Shell title="Shell" paths=icons::SHELL over />
                <Entry page=Page::Preferences title="Preferences" paths=icons::SETTINGS over />
            </div>
            <div class=style::part>
                <span>"Data"</span>
                <Entry page=Page::files() title="Files" paths=icons::FOLDER over />
                <Entry page=Page::database() title="Database" paths=icons::DATABASE over />
                <Entry page=Page::Media title="Media" paths=icons::IMAGE over />
                <Entry page=Page::Clipboard title="Clipboard" paths=icons::CLIPBOARD over />
            </div>
            <div class=style::part>
                <span>"Sensors"</span>
                <Entry page=Page::Camera title="Camera" paths=icons::CAMERA over />
                <Entry page=Page::Location title="Location" paths=icons::PIN over />
            </div>
            <div class=classes!(style::part, style::setup)>
                {move || {
                    let label = if setup_down.get() {
                        "Move System back up"
                    } else {
                        "Move System to the bottom"
                    };

                    view! {
                        <button
                            type="button"
                            class=style::split
                            title=label
                            aria-label=label
                            aria-pressed=setup_down.get().to_string()
                            on:click=move |_| setup_down.set(!setup_down.get_untracked())
                        ></button>
                    }
                }}
                <span>"System"</span>
                <Entry page=Page::Settings title="Settings" paths=icons::GEAR over />
                <Entry page=Page::Access title="Access" paths=icons::TOKEN over />
                <Entry page=Page::Network title="Network" paths=icons::NETWORK over />
                <Entry page=Page::Events title="Events" paths=icons::CLOCK over />
            </div>
            <Foot menu />
        </nav>
        <button
            type="button"
            class=style::menu_scrim
            class=(style::over, move || fold.get() == Fold::Over)
            title="Close the menu"
            aria-label="Close the menu"
            on:click=move |_| over.set(false)
        ></button>
    }
}

#[component]
fn Entry(
    page: Page,
    title: &'static str,
    paths: &'static str,
    over: RwSignal<bool>,
) -> impl IntoView {
    let Global { link, router, .. } = expect_context::<Global>();
    let camera = matches!(page, Page::Camera);
    let access = matches!(page, Page::Access);
    let needed = page.offered();
    let open = {
        let page = page.clone();

        Memo::new(move |_| router.on(|open| open.is(&page)))
    };

    let sent = move || {
        camera
            && router
                .device()
                .and_then(|id| link.run(&id)?.fed_by)
                .is_some_and(|feeder| link.me(&feeder.print))
    };

    let asking = move || {
        if access {
            link.server
                .access()
                .with(|access| access.as_ref().map_or(0, |access| access.asking.len()))
        } else {
            0
        }
    };

    let label = move || match (sent(), asking()) {
        (true, _) => format!("{title}. This browser is feeding the camera"),
        (false, 0) => title.to_owned(),
        (false, asking) => format!("{title}: {asking} waiting to be approved"),
    };

    view! {
        <button
            class=move || {
                classes!(
                    style::entry, open.get().then_some(style::active),
                    sent().then_some(style::recording)
                )
            }
            title=label
            aria-label=label
            aria-current=move || open.get().then_some("page")
            disabled=move || needed.is_some_and(|needed| router.shut(needed))
            on:click=move |_| {
                over.set(false);

                // Going again would move the open view from where it is in
                // itself.
                if !open.get_untracked() {
                    router.go(page.clone());
                }
            }
        >
            {move || {
                let paths = if sent() { icons::CAMERA_LIVE } else { paths };

                view! { <Icon paths /> }
            }}
            <span>{title}</span>
            <Show when=move || sent()>
                <span class=style::live>"(live)"</span>
            </Show>
            {move || {
                let asking = asking();

                (asking > 0)
                    .then(|| {
                        view! {
                            <span class=classes!(ui::style::count, style::waits) aria-hidden="true">
                                {asking}
                            </span>
                        }
                    })
            }}
        </button>
    }
}

#[component]
fn Foot(menu: Menu) -> impl IntoView {
    let ui = expect_context::<Global>().ui;
    let Menu {
        folded,
        over,
        small,
        fold,
        ..
    } = menu;

    view! {
        <div class=style::foot>
            {move || {
                if fold.get() == Fold::Icons {
                    let light = ui.theme.get().is_light();
                    let label = if light {
                        "Light theme. Switch to dark"
                    } else {
                        "Dark theme. Switch to light"
                    };

                    view! {
                        <button
                            class=style::entry
                            title=label
                            aria-label=label
                            on:click=move |_| {
                                ui.recolour(if light { Theme::Dark } else { Theme::Light })
                            }
                        >
                            <Icon paths=if light { icons::SUN } else { icons::MOON } />
                        </button>
                        <button
                            class=style::entry
                            title="Expand the menu"
                            aria-label="Expand the menu"
                            on:click=move |_| {
                                // On a narrow panel it expands over the
                                // view for now and nothing is stored.
                                if small.get_untracked() {
                                    over.set(true);
                                } else {
                                    over.set(false);
                                    folded.set(false);
                                }
                            }
                        >
                            <Icon paths=icons::UNFOLD />
                        </button>
                    }
                        .into_any()
                } else {
                    view! {
                        <span>"Theme"</span>
                        <Segmented
                            class=style::themes
                            label="Theme"
                            choices=Theme::ALL.map(|one| (one, one.name())).to_vec()
                            chosen=ui.theme
                            on_choose=move |one| ui.recolour(one)
                        />
                        <button
                            class=style::entry
                            on:click=move |_| {
                                if small.get_untracked() {
                                    over.set(false);
                                } else {
                                    folded.set(true);
                                }
                            }
                        >
                            <Icon paths=icons::FOLD />
                            <span>"Collapse"</span>
                        </button>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_narrow_panel_folds_the_menu_whatever_was_chosen() {
        assert!(Fold::of(true, false, false) == Fold::Icons);
        assert!(Fold::of(true, true, false) == Fold::Over);
        assert!(Fold::of(true, true, true) == Fold::Over);
        assert!(Fold::of(false, false, false) == Fold::Names);
        assert!(Fold::of(false, true, false) == Fold::Names);
        assert!(Fold::of(false, true, true) == Fold::Icons);
    }
}
