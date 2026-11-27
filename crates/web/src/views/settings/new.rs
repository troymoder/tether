use leptos::ev;
use leptos::prelude::*;
use protocol::{EmulatorOptions, EmulatorSettings, Graphics, InstanceId, Request};
use stylance::classes;

use super::hardware::HardwareFields;
use super::{here, options, style};
use crate::app::{Global, Wanted};
use crate::ui;
use crate::ui::action::use_action;
use crate::ui::button::{Button, Tone};
use crate::ui::dropdown::{Choice, Dropdown};
use crate::ui::loadable::{Status, Wait};
use crate::ui::segmented::Segmented;

#[derive(Clone, Copy, PartialEq)]
enum Runs {
    Android,
    Ios,
}

/// None for an Android SDK with no phone profile or no system image, since
/// no emulator can be made from it.
fn starting(android: &EmulatorOptions) -> Option<EmulatorSettings> {
    // The Pixel 6 is a real phone with enough memory. The SDK's default
    // profile looks dated and swaps.
    let pixel = android
        .profiles
        .iter()
        .find(|profile| profile.as_str() == "pixel_6");

    Some(EmulatorSettings {
        profile: pixel.or_else(|| android.profiles.first())?.clone(),
        // The SDK lists the newest image last.
        image: android.images.last()?.clone(),
        memory: 4096,
        graphics: Graphics::Gpu,
        autostart: false,
        camera: None,
    })
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn NewEmulator() -> impl IntoView {
    let link = expect_context::<Global>().link;
    // The primary is known once the page has connected. A memo, so the form
    // is not rebuilt when something else about who is trusted changes.
    let primary = Memo::new(move |_| link.primary());

    move || primary.get().map(|primary| view! { <Fresh primary /> })
}

#[component]
fn Fresh(primary: InstanceId) -> impl IntoView {
    let runs_on = RwSignal::new(primary);
    let android = options(move || runs_on.get());
    let runs = RwSignal::new(Runs::Android);

    view! {
        <Segmented
            label="What it runs"
            class=style::kinds
            choices=vec![(Runs::Android, "Android emulator"), (Runs::Ios, "iOS simulator")]
            chosen=runs
            on_choose=move |one| runs.set(one)
        />
        {move || match runs.get() {
            Runs::Ios => view! { <NewSimulator /> }.into_any(),
            Runs::Android => {
                view! {
                    <Status of=android loading=Wait::Blank let:android>
                        <NewAndroid android runs_on />
                    </Status>
                }
                    .into_any()
            }
        }}
    }
}

#[component]
fn NewAndroid(android: EmulatorOptions, runs_on: RwSignal<InstanceId>) -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();

    let Some(start) = starting(&android) else {
        return view! {
            <p class=ui::style::hint>
                "This instance's Android SDK has no phone profile or no system image."
            </p>
        }
        .into_any();
    };

    let name = RwSignal::new(String::new());
    let settings = RwSignal::new(start);
    let (creating, create) = use_action(None, move |()| {
        let name = name.get_untracked().trim().to_owned();
        let request = Request::CreateEmulator {
            name: name.clone(),
            settings: settings.get_untracked(),
        };

        async move {
            let said = link.call(request).await?;

            // Picks the new emulator as soon as tetherd lists it.
            ui.wanted.set(Some(Wanted::Emulator(name)));
            ui.adding.set(None);

            Ok(said)
        }
    });

    let submit = move |event: ev::SubmitEvent| {
        event.prevent_default();

        // tetherd only makes emulators on the primary instance.
        if Some(runs_on.get_untracked()) != untrack(|| link.primary()) {
            link.unimplemented("emulators on a linked instance");
            return;
        }

        create.run(());
    };

    view! {
        <form class=classes!(style::settings, style::fresh) on:submit=submit>
            <h2>
                <label for="name">"Name"</label>
            </h2>
            <input id="name" required placeholder="pixel" bind:value=name />
            <h2>
                <label for="runs-on">"Runs on"</label>
            </h2>
            <Status of=here() loading=Wait::Blank let:choices>
                <Dropdown
                    id="runs-on"
                    label="Runs on"
                    class=style::pick
                    choices
                    chosen=Signal::derive(move || Some(runs_on.get()))
                    on_choose=move |chosen: InstanceId| runs_on.set(chosen)
                />
            </Status>
            <HardwareFields
                android
                settings
                on_change=move |changed: EmulatorSettings| settings.set(changed)
            />
            <div class=style::foot>
                <Button on:click=move |_| ui.adding.set(None)>"Cancel"</Button>
                <Button submit=true tone=Tone::Blue busy=creating>
                    "Create"
                </Button>
            </div>
        </form>
    }
    .into_any()
}

#[component]
fn NewSimulator() -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let field = |id: &'static str, label: &'static str| {
        // No tetherd runs simulators, so there is nothing to choose.
        let none: Vec<Choice<String>> = Vec::new();
        let chosen: Option<String> = None;

        view! {
            <h2>
                <label for=id>{label}</label>
            </h2>
            <Dropdown
                id=id
                label=label
                class=style::pick
                placeholder="No tetherd has the part for it"
                choices=none
                chosen
                on_choose=|_| ()
            />
        }
    };

    view! {
        <form
            class=classes!(style::settings, style::fresh)
            on:submit=move |event| {
                event.prevent_default();
                link.unimplemented("iOS simulators");
            }
        >
            <h2>
                <label for="simulator-name">"Name"</label>
            </h2>
            <input id="simulator-name" required placeholder="iphone" />
            {field("simulator-runs-on", "Runs on")}
            {field("simulator-phone", "Phone")}
            {field("simulator-system", "System")}
            <div class=style::foot>
                <Button on:click=move |_| ui.adding.set(None)>"Cancel"</Button>
                <Button submit=true tone=Tone::Blue>
                    "Create"
                </Button>
            </div>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_emulator_starts_as_a_pixel_6_on_the_newest_image() {
        let android = EmulatorOptions {
            profiles: vec!["nexus_5".to_owned(), "pixel_6".to_owned()],
            images: vec!["android-35".to_owned(), "android-36".to_owned()],
            feeds_cameras: false,
        };
        let start = starting(&android).map(|start| (start.profile, start.image, start.graphics));
        let no_pixel = EmulatorOptions {
            profiles: vec!["nexus_5".to_owned()],
            ..android.clone()
        };
        let no_image = EmulatorOptions {
            images: Vec::new(),
            ..android
        };

        assert_eq!(
            start,
            Some(("pixel_6".to_owned(), "android-36".to_owned(), Graphics::Gpu))
        );
        assert_eq!(
            starting(&no_pixel).map(|start| start.profile),
            Some("nexus_5".to_owned())
        );
        assert_eq!(starting(&no_image), None);
    }
}
