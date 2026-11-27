use leptos::prelude::*;
use protocol::{App, DeviceId, Request, Variant};

use super::{platforms, style};
// Renamed because `Page` in this module is the page of one app.
use crate::app::{Global, Page as Open};
use crate::link::ServerStoreFields;
use crate::ui;
use crate::ui::button::{Button, Tone};
use crate::ui::menu::use_menu;
use crate::ui::pane::{Crumb, Pane};
use crate::views::apps::icon::{AppIcon, use_registry_icon};
use crate::views::apps::{Features, Installed, called, use_app};

mod build;
mod install_menu;
mod installed_on;

use build::BuildRow;
use install_menu::InstallMenu;
use installed_on::InstalledOn;

/// The memo is rebuilt whenever the asked devices change, so read it each time
/// and never keep a copy.
type Asked = Memo<Vec<(DeviceId, Installed)>>;

fn of(asked: &[(DeviceId, Installed)], device: &DeviceId) -> Option<Installed> {
    let (_, installed) = asked.iter().find(|(id, _)| id == device)?;

    Some(*installed)
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Page(app: App) -> impl IntoView {
    let Global { link, router, ui } = expect_context::<Global>();
    let id = app.id.clone();
    let app = use_app(app);
    let drawn = use_registry_icon(move || Some(app.get()));
    let menu = use_menu::<u32>();
    // The memo only changes when the list of devices does, so other state
    // changes do not ask the devices again.
    let devices = Memo::new(move |_| {
        let ready: Vec<DeviceId> = link.server.devices().with(|devices| {
            app.with(|app| {
                devices
                    .iter()
                    .filter(|device| device.ready())
                    .filter(|device| platforms(app).contains(&device.id.platform()))
                    .map(|device| device.id.clone())
                    .collect()
            })
        });

        ready
            .into_iter()
            .filter(|id| {
                crate::views::apps::Apps::features(&*crate::device::of(link, id.clone()))
                    .contains(Features::MANAGES)
            })
            .collect::<Vec<_>>()
    });
    // Each run starts a fresh poll of every device. The compare always
    // reports a change, so no reader keeps an `Installed` from an older run.
    let asked: Asked = Memo::new_with_compare(
        move |_| {
            devices
                .get()
                .into_iter()
                .map(|device| (device.clone(), Installed::of(device)))
                .collect::<Vec<_>>()
        },
        |_, _| true,
    );
    // A build without native code has no ABI and gets one row.
    let builds = Memo::new(move |_| {
        let mut rows: Vec<(Variant, Option<String>)> = app.with(|app| {
            app.variants
                .iter()
                .flat_map(|variant| {
                    let abis: Vec<Option<String>> = if variant.abis.is_empty() {
                        vec![None]
                    } else {
                        variant.abis.iter().cloned().map(Some).collect()
                    };

                    abis.into_iter().map(|abi| (variant.clone(), abi))
                })
                .collect()
        });

        rows.sort_by(|one, other| other.0.build.cmp(&one.0.build).then(one.1.cmp(&other.1)));

        rows
    });
    let remove = {
        let id = id.clone();

        move |_| {
            let id = id.clone();

            ui.confirm_then(
                format!("Remove {id} from the registry?"),
                "Its builds go. It stays on the devices it is installed on.",
                "Remove",
                async move {
                    link.ask(Request::ForgetApp { id });
                    router.go(Open::registry());
                },
            );
        }
    };
    let app_id = StoredValue::new(id.clone());

    view! {
        <Pane
            title=Signal::derive(move || app.with(called))
            crumbs=Crumb {
                name: "Registry",
                back: Callback::new(move |()| router.go(Open::registry())),
            }
        >
            <div class=ui::style::pane_body>
                <div class=style::head>
                    <AppIcon id=id.clone() drawn />
                    <span class=style::names>
                        <strong>{move || app.with(called)}</strong>
                        <code>{id}</code>
                    </span>
                    <Button tone=Tone::Red quiet=true class=style::remove on:click=remove>
                        "Remove from registry"
                    </Button>
                </div>
                <h2>"Builds"</h2>
                <div class=style::table>
                    <div class=style::builds>
                        <For
                            each=move || builds.get()
                            key=|(variant, abi)| (variant.build, abi.clone())
                            children=move |(variant, abi)| {
                                let build = variant.build;
                                let latest = Signal::derive(move || {
                                    app.with(|app| app.latest().map(|latest| latest.build))
                                        == Some(build)
                                });

                                view! {
                                    <BuildRow app=app_id.get_value() variant abi latest menu />
                                }
                            }
                        />
                    </div>
                </div>
                <h2 class=style::later>"Installed on"</h2>
                <InstalledOn app asked />
            </div>
            <InstallMenu menu app asked />
        </Pane>
    }
}
