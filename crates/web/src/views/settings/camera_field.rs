use leptos::prelude::*;
use protocol::{CameraSettings, Lens, VirtualCamera};

use super::style;
use crate::app::{Icon, icons};
use crate::ui::dropdown::Choice;
use crate::ui::popup::{self, Popup, Radios};

#[allow(unreachable_pub)]
#[component]
pub(super) fn CameraField(
    #[prop(into)] camera: Signal<Option<VirtualCamera>>,
    /// True when tetherd has the shim, which makes virtual cameras.
    can: bool,
    #[prop(into)] on_change: Callback<Option<VirtualCamera>>,
) -> impl IntoView {
    let lens_name = |lens: Lens| match lens {
        Lens::Back => "Back",
        Lens::Front => "Front",
    };

    let summary = move || match camera.get() {
        None => "Off".to_owned(),
        Some(VirtualCamera { lens, settings }) => format!(
            "{} · {} × {} · {} fps",
            lens_name(lens),
            settings.width,
            settings.height,
            settings.fps
        ),
    };

    let chosen = move || camera.get().map(|camera| camera.settings);
    let set = move |change: &dyn Fn(&mut CameraSettings)| {
        if let Some(mut camera) = camera.get_untracked() {
            change(&mut camera.settings);
            on_change.run(Some(camera));
        }
    };

    let lenses: Vec<Choice<Option<Lens>>> = vec![
        Choice::new(None, "Off"),
        Choice {
            disabled: !can,
            ..Choice::new(Some(Lens::Back), "Back")
        },
        Choice {
            disabled: !can,
            ..Choice::new(Some(Lens::Front), "Front")
        },
    ];
    let sizes: Vec<Choice<(u16, u16)>> = protocol::CAMERA_SIZES
        .iter()
        .map(|size| Choice::new(*size, format!("{} × {}", size.0, size.1)))
        .collect();
    let rates: Vec<Choice<u8>> = protocol::CAMERA_RATES
        .iter()
        .map(|fps| Choice::new(*fps, format!("{fps} fps")))
        .collect();
    let choose_lens = move |chosen: Option<Lens>| {
        // Switching lenses keeps the size and rate. A camera being turned on
        // starts with that lens's usual settings.
        let changed = chosen.map(|lens| VirtualCamera {
            lens,
            settings: camera
                .get_untracked()
                .map_or_else(|| protocol::usual_camera(lens), |before| before.settings),
        });

        on_change.run(changed);
    };

    view! {
        <div>
            <Popup
                button=move || {
                    view! {
                        <span class=style::chosen>{summary}</span>
                        <Icon paths=icons::CHEVRON />
                    }
                }
                button_class=style::popup_field
                id="camera"
                label="Camera"
                class=style::camera_box
                as_wide=true
            >
                <Radios
                    heading="Lens"
                    choices=lenses.clone()
                    chosen=Signal::derive(move || Some(camera.get().map(|camera| camera.lens)))
                    on_choose=choose_lens
                />
                <span class=popup::style::rule aria-hidden="true"></span>
                <Radios
                    heading="Size"
                    choices=sizes.clone()
                    disabled=Signal::derive(move || camera.get().is_none())
                    chosen=Signal::derive(move || {
                        chosen().map(|settings| (settings.width, settings.height))
                    })
                    on_choose=move |(width, height): (u16, u16)| {
                        set(
                            &|settings| {
                                settings.width = width;
                                settings.height = height;
                            },
                        );
                    }
                />
                <span class=popup::style::rule aria-hidden="true"></span>
                <Radios
                    heading="Frame rate"
                    choices=rates.clone()
                    side_by_side=true
                    disabled=Signal::derive(move || camera.get().is_none())
                    chosen=Signal::derive(move || chosen().map(|settings| settings.fps))
                    on_choose=move |fps: u8| set(&|settings| settings.fps = fps)
                />
            </Popup>
        </div>
    }
}
