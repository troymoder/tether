use leptos::prelude::*;
use protocol::{App, DeviceId};

use super::super::style;
use super::{Asked, of};
use crate::app::Global;
use crate::ui::auto_toggle::AutoToggle;
use crate::ui::button::{Button, Tone};
use crate::ui::loadable::Loadable;
use crate::ui::logo::platform_logo;
use crate::ui::tag::{Dot, Tag};
use crate::views::apps::installed::Standing;
use crate::views::apps::status::StatusTag;
use crate::views::apps::style as apps;

#[allow(unreachable_pub)]
#[component]
pub(super) fn InstalledOn(app: Memo<App>, asked: Asked) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let devices = Memo::new(move |_| {
        let asked = asked.get();

        app.with(|app| {
            let mut devices = app.auto.clone();

            for (device, installed) in &asked {
                let has = match installed.standing(app) {
                    Loadable::Ready(standing) => standing != Standing::Absent,
                    Loadable::Loading | Loadable::Failed(_) => false,
                };

                if has && !devices.contains(device) {
                    devices.push(device.clone());
                }
            }

            devices.sort_by_cached_key(|device| link.name(device).to_lowercase());

            devices
        })
    });

    view! {
        <Show
            when=move || !devices.with(Vec::is_empty)
            fallback=|| view! { <span class=style::note>"Not installed on any device."</span> }
        >
            <div class=apps::grid>
                <For each=move || devices.get() key=DeviceId::clone let:device>
                    <InstalledOnRow app device asked />
                </For>
            </div>
        </Show>
    }
}

#[allow(unreachable_pub)]
#[component]
fn InstalledOnRow(app: Memo<App>, device: DeviceId, asked: Asked) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let logo = platform_logo(device.platform());
    let id = app.with_untracked(|app| app.id.clone());
    let device = StoredValue::new(device);
    let name = move || device.with_value(|device| link.name(device));
    // A device that only auto-updates the app may be disconnected, and is
    // not asked.
    let installed = move || device.with_value(|device| asked.with(|asked| of(asked, device)));
    let standing =
        Memo::new(move |_| installed().map(|installed| app.with(|app| installed.standing(app))));
    let state = move || match standing.get() {
        Some(standing) => view! { <StatusTag standing /> }.into_any(),
        None => {
            let ready = device.with_value(|device| link.device(device));
            let words = if ready.as_ref().is_some_and(protocol::Device::ready) {
                "Not installed"
            } else {
                "Not connected"
            };

            view! {
                <Tag tone=Dot::Ring title=words>
                    <span class=apps::words>{words}</span>
                </Tag>
            }
            .into_any()
        }
    };
    let runs = move || installed().and_then(|installed| app.with(|app| installed.build(app)));
    let behind = move || {
        matches!(
            standing.get(),
            Some(Loadable::Ready(Standing::Behind | Standing::Failed(_)))
        )
    };
    let install = move |_| {
        let installed = device.with_value(|device| asked.with_untracked(|asked| of(asked, device)));

        if let Some(installed) = installed {
            app.with_untracked(|app| installed.install(app));
        }
    };

    view! {
        <div class=apps::row>
            <span class=style::device_logo>{logo}</span>
            <span class=apps::names>
                <span class=apps::title>
                    <strong>{name}</strong>
                </span>
                {move || {
                    runs()
                        .map(|runs| {
                            view! { <code class=style::on_build>{format!("build {runs}")}</code> }
                        })
                }}
            </span>
            <span class=apps::state_cell>{state}</span>
            <span>
                <AutoToggle app=id device=device.get_value() />
            </span>
            <span class=apps::actions>
                // An older registry build reads as Update, and an install
                // from elsewhere as Reinstall.
                <Show when=behind>
                    <Button tone=Tone::Blue small=true on:click=install>
                        {move || if runs().is_some() { "Update" } else { "Reinstall" }}
                    </Button>
                </Show>
            </span>
        </div>
    }
}
