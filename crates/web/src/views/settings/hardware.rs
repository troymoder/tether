use leptos::prelude::*;
use protocol::{EmulatorOptions, EmulatorSettings, Graphics, VirtualCamera};

use super::camera_field::CameraField;
use super::{image_name, style};
use crate::ui::dropdown::{Choice, Dropdown};

#[allow(unreachable_pub)]
#[component]
pub(super) fn HardwareFields(
    android: EmulatorOptions,
    #[prop(into)] settings: Signal<EmulatorSettings>,
    #[prop(into)] on_change: Callback<EmulatorSettings>,
) -> impl IntoView {
    let choices = |all: &[String], label: fn(&str) -> String| -> Vec<Choice<String>> {
        all.iter()
            .map(|one| Choice::new(one.clone(), label(one)))
            .collect()
    };

    let profiles = choices(&android.profiles, str::to_owned);
    let images = choices(&android.images, image_name);
    let memories: Vec<Choice<u32>> = [2048, 4096, 6144, 8192]
        .into_iter()
        .map(|megabytes: u32| Choice::new(megabytes, format!("{} GB", megabytes / 1024)))
        .collect();
    let graphics = vec![
        Choice::new(Graphics::Gpu, "Prefer hardware"),
        Choice::new(Graphics::Software, "Software"),
    ];
    let choose_camera = move |camera: Option<VirtualCamera>| {
        on_change.run(EmulatorSettings {
            camera,
            ..settings.get_untracked()
        });
    };

    view! {
        <h2>
            <label for="profile">"Phone"</label>
        </h2>
        <Dropdown
            id="profile"
            label="Phone"
            class=style::pick
            choices=profiles
            chosen=Signal::derive(move || Some(settings.with(|settings| settings.profile.clone())))
            on_choose=move |profile: String| {
                on_change
                    .run(EmulatorSettings {
                        profile,
                        ..settings.get_untracked()
                    });
            }
        />

        <h2>
            <label for="image">"System image"</label>
        </h2>
        <Dropdown
            id="image"
            label="System image"
            class=style::pick
            choices=images
            chosen=Signal::derive(move || Some(settings.with(|settings| settings.image.clone())))
            on_choose=move |image: String| {
                on_change
                    .run(EmulatorSettings {
                        image,
                        ..settings.get_untracked()
                    });
            }
        />

        <h2>
            <label for="memory">"Memory"</label>
        </h2>
        <Dropdown
            id="memory"
            label="Memory"
            class=style::pick
            choices=memories
            chosen=Signal::derive(move || Some(settings.with(|settings| settings.memory)))
            on_choose=move |memory: u32| {
                on_change
                    .run(EmulatorSettings {
                        memory,
                        ..settings.get_untracked()
                    });
            }
        />

        <h2>
            <label for="graphics">"Graphics"</label>
        </h2>
        <Dropdown
            id="graphics"
            label="Graphics"
            class=style::pick
            choices=graphics
            chosen=Signal::derive(move || Some(settings.with(|settings| settings.graphics)))
            on_choose=move |graphics: Graphics| {
                on_change
                    .run(EmulatorSettings {
                        graphics,
                        ..settings.get_untracked()
                    });
            }
        />

        <h2>
            <label for="camera">"Camera"</label>
        </h2>
        <CameraField
            camera=Signal::derive(move || settings.with(|settings| settings.camera))
            can=android.feeds_cameras
            on_change=choose_camera
        />
    }
}
