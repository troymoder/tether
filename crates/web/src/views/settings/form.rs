//! The fields show what tetherd has, and each change is saved as it is made,
//! with no save button.

use leptos::ev;
use leptos::prelude::*;
use protocol::{EmulatorOptions, EmulatorSettings, Request, Status};
use stylance::classes;

use super::hardware::HardwareFields;
use super::style;
use crate::app::Global;
use crate::ui;
use crate::ui::button::{Button, Tone};
use crate::ui::name_field::NameField;
use crate::views::devices::delete;

#[allow(unreachable_pub)]
#[component]
pub(crate) fn EmulatorForm(
    avd: String,
    android: EmulatorOptions,
    #[prop(into)] settings: Signal<EmulatorSettings>,
) -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let avd = StoredValue::new(avd);
    let device = Memo::new(move |_| avd.with_value(|avd| link.emulator(avd)));
    // An emulator starts with its settings, so they stay locked until it has
    // stopped.
    let running = Memo::new(move |_| {
        device.with(|device| {
            let run = device.as_ref().and_then(|device| device.emulator.as_ref());

            run.is_some_and(|run| run.status != Status::Shutdown)
        })
    });
    // The name falls back to the virtual device name in the moment after
    // tetherd stops listing the emulator.
    let name = Signal::derive(move || {
        device.with(|device| match device {
            Some(device) => device.name.clone(),
            None => avd.get_value(),
        })
    });
    let rename = move |name: String| {
        // The id is read at rename time, since an emulator's id holds the
        // port it runs on.
        if let Some(device) = device.get_untracked() {
            link.ask(Request::RenameDevice {
                device: device.id,
                name,
            });
        }
    };

    // A change applies to the settings tetherd last sent, so of two changes
    // within one round trip only the second is kept.
    let save = move |changed: EmulatorSettings| {
        let before = settings.get_untracked();

        if changed == before {
            return;
        }

        let name = avd.get_value();
        let title = format!("Change {name}?");
        // A different phone profile or system image wipes the emulator, so
        // the user is asked first.
        let wipes = changed.profile != before.profile || changed.image != before.image;
        let request = Request::Configure {
            name,
            settings: changed,
        };

        if wipes {
            ui.confirm_then(
                title,
                "A different phone or system image wipes what is on it the next time it starts.",
                "Change",
                async move { link.ask(request) },
            );
        } else {
            link.ask(request);
        }
    };

    let start_with = move |event: ev::Event| {
        save(EmulatorSettings {
            autostart: event_target_checked(&event),
            ..settings.get_untracked()
        });
    };

    // Erase and Delete are disabled while the emulator runs, since tetherd
    // refuses both then.
    let stop_first = move || running.get().then_some("Stop it first");

    view! {
        <form class=style::settings on:submit=|event| event.prevent_default()>
            <h2>"Name"</h2>
            <div class=style::name>
                <NameField name renamed=rename />
            </div>
            <h2 class=style::part>"Hardware"</h2>
            <Show when=move || running.get()>
                <p class=classes!(style::caution, style::warn)>
                    {format!("{} is running. Stop it to change these.", avd.get_value())}
                </p>
            </Show>
            <fieldset disabled=running>
                <HardwareFields android settings on_change=save />
                <label class=ui::style::switched>
                    <input
                        type="checkbox"
                        class=ui::style::switch
                        prop:checked=move || settings.with(|settings| settings.autostart)
                        on:change=start_with
                    />
                    "Start with tetherd"
                </label>
            </fieldset>

            <div class=style::foot>
                <Button
                    disabled=running
                    attr:title=stop_first
                    on:click=move |_| {
                        let name = avd.get_value();

                        ui.confirm_then(
                            format!("Erase {name}?"),
                            "Wipes everything on it. Its settings stay.",
                            "Erase",
                            async move { link.ask(Request::Erase { name }) },
                        );
                    }
                >
                    "Erase"
                </Button>
                <Button
                    tone=Tone::Red
                    disabled=running
                    attr:title=stop_first
                    on:click=move |_| {
                        // The emulator's page closes by itself once tetherd
                        // stops listing it.
                        if let Some(device) = device.get_untracked() {
                            delete(ui, link, &device);
                        }
                    }
                >
                    "Delete"
                </Button>
            </div>
        </form>
    }
}
