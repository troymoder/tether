use leptos::ev;
use leptos::prelude::*;
use protocol::App;

use super::super::installed::{Installed, Standing};
use super::super::style;
use super::Entry;
use crate::app::Picked;
use crate::ui::action::use_action;
use crate::ui::button::{Button, Tone};
use crate::ui::loadable::Loadable;
use crate::ui::menu::MenuOf;

#[allow(unreachable_pub)]
#[component]
pub(super) fn RowActions(
    entry: Entry,
    app: Option<Memo<App>>,
    installed: Installed,
    menu: MenuOf<Entry>,
    running: Signal<bool>,
    launchable: Signal<bool>,
) -> impl IntoView {
    let device = expect_context::<Picked>().device;
    let id = StoredValue::new(entry.package.id.clone());
    let name = entry.name();
    // Nothing else is done with an app while a build of it goes on.
    let installing = Memo::new(move |_| {
        app.is_some_and(|app| {
            app.with(|app| installed.standing(app)) == Loadable::Ready(Standing::Installing)
        })
    });
    let opened = format!("Opened {name}");
    let (opening, open) = use_action(Some("apps"), move |()| {
        let device = device.get_untracked();
        let id = id.get_value();
        let opened = opened.clone();

        async move {
            let done = device.open(&id).await;

            installed.refresh();

            done.map(|()| opened)
        }
    });
    let stopped = format!("Stopped {name}");
    let (stopping, stop) = use_action(Some("apps"), move |()| {
        let device = device.get_untracked();
        let id = id.get_value();
        let stopped = stopped.clone();

        async move {
            let done = device.stop(&id).await;

            installed.refresh();

            done.map(|()| stopped)
        }
    });
    let more = move |event: ev::MouseEvent| menu.under(&event, entry.clone());

    view! {
        <span class=style::actions>
            {app.map(|app| view! { <Update app installed /> })}
            <Button
                small=true
                busy=opening
                disabled=Signal::derive(move || installing.get() || !launchable.get())
                attr:title=move || {
                    if launchable.get() {
                        "Open, or bring it to the front"
                    } else {
                        "Nothing to open"
                    }
                }
                on:click=move |_| open.run(())
            >
                "Open"
            </Button>
            <Button
                tone=Signal::derive(move || if running.get() { Tone::Red } else { Tone::Plain })
                small=true
                busy=stopping
                disabled=Signal::derive(move || installing.get() || !running.get())
                attr:title=move || (!running.get()).then_some("Not running")
                on:click=move |_| stop.run(())
            >
                "Stop"
            </Button>
            <Button
                quiet=true
                small=true
                class=style::more
                attr:aria-label=format!("More for {name}")
                attr:aria-haspopup="menu"
                on:click=more
            >
                "⋯"
            </Button>
        </span>
    }
}

#[allow(unreachable_pub)]
#[component]
fn Update(app: Memo<App>, installed: Installed) -> impl IntoView {
    let standing = Memo::new(move |_| app.with(|app| installed.standing(app)));
    // A build that failed to install can be tried again.
    let offered = move || {
        matches!(
            standing.get(),
            Loadable::Ready(Standing::Behind | Standing::Installing | Standing::Failed(_))
        )
    };
    let installing =
        Signal::derive(move || standing.get() == Loadable::Ready(Standing::Installing));
    let words = move || {
        if app.with(|app| installed.build(app)).is_some() {
            "Update"
        } else {
            "Reinstall"
        }
    };

    view! {
        <Show when=offered>
            <Button
                tone=Tone::Blue
                small=true
                busy=installing
                on:click=move |_| app.with_untracked(|app| installed.install(app))
            >
                {words}
            </Button>
        </Show>
    }
}
