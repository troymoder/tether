use leptos::prelude::*;
use stylance::classes;

use super::line::Level;
use super::sift::{Of, Sift};
use super::style;
use crate::app::{Global, Icon, icons};
use crate::link::ServerStoreFields;
use crate::ui;
use crate::ui::button::{self, Button};
use crate::ui::dropdown::{Choice, Dropdown};
use crate::ui::popup::{self, Popup};

#[allow(unreachable_pub)]
#[component]
pub(super) fn FilterPopup(
    sift: Sift,
    #[prop(into)] running: Signal<Vec<String>>,
    /// True while the emulator's own output is in view, which has neither
    /// levels to choose from nor apps.
    #[prop(into)]
    disabled: Signal<bool>,
    #[prop(into)] wrap: Signal<bool>,
    #[prop(into)] on_wrap: Callback<bool>,
    #[prop(into)] on_clear: Callback<()>,
) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let Sift {
        least, of, front, ..
    } = sift;

    // The app filter names the registered apps.
    link.follow(|| Some(protocol::Topic::Apps));

    let apps = Memo::new(move |_| {
        let registered: Vec<String> = link
            .server
            .apps()
            .with(|apps| apps.iter().map(|app| app.id.clone()).collect());
        let running = running.get();
        let others = running.iter().filter(|name| !registered.contains(name));
        let in_front = match front.get() {
            Some(front) => format!("Front app · {front}"),
            None => "Front app".to_owned(),
        };

        [
            Choice::new(Of::Front, in_front),
            Choice::new(Of::All, "All apps"),
        ]
        .into_iter()
        .chain(
            registered
                .iter()
                .chain(others)
                .map(|name| Choice::new(Of::App(name.clone()), name)),
        )
        .collect::<Vec<_>>()
    });
    let levels = vec![
        Choice::new(Level::Verbose, "Verbose"),
        Choice::new(Level::Debug, "Debug"),
        Choice::new(Level::Info, "Info"),
        Choice::new(Level::Warning, "Warn"),
        Choice::new(Level::Error, "Error"),
    ];
    let label = move || {
        if disabled.get() {
            return "The emulator's own output is not filtered".to_owned();
        }

        let whose = match of.get() {
            Of::Front => match front.get() {
                Some(front) => format!("the app at the front ({front})"),
                None => "the app at the front".to_owned(),
            },
            Of::All => "all apps".to_owned(),
            Of::App(app) => app,
        };

        format!(
            "Which lines are shown: {} at least, {whose}",
            least.get().name()
        )
    };

    view! {
        <span class=style::cut>
            <Popup
                button=|| view! { <Icon paths=icons::FILTER /> }
                button_class=classes!(button::style::icon, button::style::framed)
                button_label=Signal::derive(label)
                lit=Signal::derive(move || {
                    least.get() != Level::Info || of.with(|of| *of != Of::Front)
                })
                disabled
                label="Which lines are shown"
                class=style::cut_box
                right=true
            >
                <span class=popup::style::heading>"Minimum Level"</span>
                <Dropdown
                    label="Minimum level"
                    choices=levels.clone()
                    chosen=Signal::derive(move || Some(least.get()))
                    on_choose=move |level: Level| least.set(level)
                />
                <span class=popup::style::heading>"App"</span>
                <Dropdown
                    label="Whose lines are shown"
                    choices=apps
                    chosen=Signal::derive(move || Some(of.get()))
                    on_choose=move |chosen: Of| of.set(chosen)
                />
                <span class=popup::style::rule aria-hidden="true"></span>
                <div class=popup::style::rest>
                    <label
                        class=classes!(ui::style::switched, style::wrap)
                        title="A line that is too long goes on under itself"
                    >
                        <input
                            type="checkbox"
                            class=ui::style::switch
                            prop:checked=move || wrap.get()
                            on:change=move |event| on_wrap.run(event_target_checked(&event))
                        />
                        "Wrap"
                    </label>
                    <Button quiet=true on:click=move |_| on_clear.run(())>
                        "Clear the log"
                    </Button>
                </div>
            </Popup>
        </span>
    }
}
