use android::scrcpy::Camera;
use leptos::prelude::*;
use protocol::CameraMode;

use super::mode::{Shown, area, aspect, camera_name, rates_at, size_name};
use super::style;
use crate::app::{Icon, icons};
use crate::ui::chips::{Chip, Chips};
use crate::ui::dropdown::{Choice, Dropdown};
use crate::ui::popbox::PopBox;

const USUAL_RATE: u32 = 30;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Pick(
    cameras: Vec<Camera>,
    /// None until this browser has read the mode it kept.
    #[prop(into)]
    shown: Signal<Option<Shown>>,
    #[prop(into)] on_camera: Callback<String>,
    #[prop(into)] on_mode: Callback<CameraMode>,
    open: RwSignal<bool>,
) -> impl IntoView {
    let size = Signal::derive(move || {
        shown.with(|shown| Some(shown.as_ref()?.mode.as_ref()?.size.clone()))
    });
    let rate =
        Signal::derive(move || shown.with(|shown| Some(shown.as_ref()?.mode.as_ref()?.rate)));
    let label = move || {
        shown.with(|shown| {
            shown.as_ref().map(|shown| match &shown.mode {
                Some(mode) => format!(
                    "{} · {} · {} fps",
                    camera_name(&shown.camera),
                    size_name(&mode.size),
                    mode.rate
                ),
                None => camera_name(&shown.camera),
            })
        })
    };

    let sizes: Signal<Vec<Choice<String>>> = Signal::derive(move || {
        shown.with(|shown| {
            let Some(Shown { camera, .. }) = shown else {
                return Vec::new();
            };

            let mut sizes: Vec<&String> = camera.sizes.iter().collect();

            sizes.sort_by_key(|size| std::cmp::Reverse(area(size)));
            sizes
                .into_iter()
                .map(|size| {
                    let fast = camera.fast.iter().any(|(one, _)| one == size);
                    let choice = Choice::new(size.clone(), size_name(size));

                    match (aspect(size), fast) {
                        (Some(aspect), true) => choice.noted(format!("{aspect} · high speed")),
                        (Some(aspect), false) => choice.noted(aspect),
                        (None, true) => choice.noted("high speed"),
                        (None, false) => choice,
                    }
                })
                .collect()
        })
    });

    let rates = Signal::derive(move || {
        shown.with(|shown| {
            let Some(Shown { camera, mode }) = shown else {
                return Vec::new();
            };

            let fast: &[u32] = if camera.fast.is_empty() {
                &[]
            } else {
                &[120, 240]
            };
            // Without a size there is nothing to hold a rate against.
            let can = mode
                .as_ref()
                .map(|mode| rates_at(camera, &mode.size))
                .unwrap_or_default();
            let fast_sizes = camera
                .fast
                .iter()
                .map(|(size, _)| size_name(size))
                .collect::<Vec<_>>()
                .join(" and ");

            camera
                .rates
                .iter()
                .chain(fast)
                .map(|&one| Chip {
                    unable: (!can.contains(&one)).then(|| format!("High speed: {fast_sizes} only")),
                    ..Chip::new(one, format!("{one} fps"))
                })
                .collect::<Vec<_>>()
        })
    });

    let choices = move || {
        cameras
            .iter()
            .map(|one| {
                let id = one.id.clone();
                let note = format!("camera {} · {}", one.id, size_name(&one.size));
                let name = camera_name(one);
                let of = id.clone();
                let on = move || {
                    shown.with(|shown| shown.as_ref().is_some_and(|shown| shown.camera.id == of))
                };
                let selected = on.clone();

                view! {
                    <button
                        type="button"
                        role="option"
                        aria-selected=move || selected().to_string()
                        on:click=move |_| {
                            if !untrack(&on) {
                                on_camera.run(id.clone());
                            }
                        }
                    >
                        <span>{name}</span>
                        <span class=style::note>{note}</span>
                    </button>
                }
            })
            .collect_view()
    };

    view! {
        <span class=style::pick>
            <button
                type="button"
                class=style::source
                aria-label=move || {
                    match label() {
                        Some(label) => format!("Which camera of the device: {label}"),
                        None => "Which camera of the device".to_owned(),
                    }
                }
                aria-haspopup="dialog"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|open| *open = !*open)
            >
                <span class=style::what>{label}</span>
                <Icon paths=icons::CHEVRON />
            </button>
            <PopBox open label="The device's camera" class=style::sources>
                <div role="listbox" aria-label="Camera" class=style::kinds>
                    {choices.clone()}
                </div>
                <div class=style::settings>
                    <span class=style::heading>"Size"</span>
                    <div class=style::field>
                        <Dropdown
                            class=style::size
                            label="Size"
                            choices=sizes
                            chosen=size
                            on_choose=move |size: String| {
                                let Some(now) = shown.get_untracked() else {
                                    return;
                                };

                                let rate = now
                                    .mode
                                    .map(|mode| mode.rate)
                                    .filter(|rate| rates_at(&now.camera, &size).contains(rate))
                                    .unwrap_or(USUAL_RATE);

                                on_mode.run(CameraMode { size, rate });
                            }
                        />
                    </div>
                    <Chips
                        heading="Frame rate"
                        chips=rates
                        chosen=rate
                        on_choose=move |rate: u32| {
                            if let Some(size) = size.get_untracked() {
                                on_mode.run(CameraMode { size, rate });
                            }
                        }
                        class=style::rates
                    />
                </div>
            </PopBox>
        </span>
    }
}
