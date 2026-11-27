use leptos::prelude::*;
use protocol::{Device, DeviceId, Topic};
use stylance::classes;

use super::card::Card;
use super::listed::{self, Kind};
use super::menu::menu_items;
use super::states::StateFilter;
use super::style;
use crate::app::{Adding, Global};
use crate::app::{Icon, icons};
use crate::link::ServerStoreFields;
use crate::ui;
use crate::ui::button::Button;
use crate::ui::menu::{MenuOf, use_menu};
use crate::ui::pane::Pane;
use crate::ui::renamable::Renaming;
use crate::ui::search::{Empty, SearchBox};

#[allow(unreachable_pub)]
#[component]
pub(super) fn List() -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let sought = RwSignal::new(String::new());
    let hidden = RwSignal::new(Vec::<Kind>::new());
    let menu = use_menu::<DeviceId>();
    let renaming = Renaming::<DeviceId>::new();
    let emulators = Memo::new(move |_| {
        link.server.devices().with(|devices| {
            devices
                .iter()
                .filter_map(|device| device.emulator.as_ref())
                .map(|run| run.avd.clone())
                .collect::<Vec<_>>()
        })
    });

    link.follow(move || emulators.get().into_iter().map(Topic::Emulator));

    let all = Memo::new(move |_| {
        let mut devices = link.server.devices().get();

        devices.sort_by_key(listed::rank);

        let (emulators, devices): (Vec<Device>, Vec<Device>) = devices
            .into_iter()
            .partition(|device| device.emulator.is_some());
        let (found, others): (Vec<Device>, Vec<Device>) =
            devices.into_iter().partition(listed::new);

        [emulators, others, found]
    });

    let shown = Memo::new(move |_| {
        let sought = sought.get();
        let hidden = hidden.get();

        all.get().map(|part| {
            part.into_iter()
                .filter(|device| listed::matches(device, &sought, &hidden))
                .collect::<Vec<_>>()
        })
    });
    // No more columns than the longest section has cards, so cards are not
    // narrowed for empty columns.
    let most = Memo::new(move |_| {
        shown.with(|shown| shown.iter().map(Vec::len).max().unwrap_or(1).max(1))
    });
    let part = move |index: usize| Signal::derive(move || shown.with(|shown| shown[index].clone()));
    let count = Callback::new(move |kind: Kind| {
        all.with(|all| {
            let devices = all.iter().flatten();

            devices
                .filter(|device| listed::tag(device).2 == kind)
                .count()
        })
    });
    let total = Signal::derive(move || all.with(|all| all.iter().map(Vec::len).sum::<usize>()));
    let left =
        Signal::derive(move || shown.with(|shown| shown.iter().map(Vec::len).sum::<usize>()));

    view! {
        <Pane
            title="Devices"
            under=move || {
                view! {
                    <div class=style::narrow>
                        <SearchBox sought label="Search devices" placeholder="Search devices" />
                        <StateFilter hidden count />
                        <Button
                            class=style::add
                            attr:title="Add device"
                            attr:aria-label="Add device"
                            on:click=move |_| ui.adding.set(Some(Adding::Any))
                        >
                            <Icon paths=icons::ADD />
                            <span>"Add device"</span>
                        </Button>
                    </div>
                }
            }
        >
            <div class=classes!(ui::style::pane_body, style::body)>
                <Empty all=total shown=left none="No devices." class=style::empty />
                <Section title="Emulators" devices=part(0) most menu renaming />
                <Section title="Other devices" devices=part(1) most menu renaming />
                <Section title="Available to connect" devices=part(2) most menu renaming />
            </div>
        </Pane>
        {menu.view(true, move |id| {
            let device = link.device(id)?;

            Some((device.name.clone(), menu_items(&device, renaming)))
        })}
    }
}

#[component]
fn Section(
    title: &'static str,
    devices: Signal<Vec<Device>>,
    #[prop(into)] most: Signal<usize>,
    menu: MenuOf<DeviceId>,
    renaming: Renaming<DeviceId>,
) -> impl IntoView {
    view! {
        <Show when=move || devices.with(|devices| !devices.is_empty())>
            <h2>{title}</h2>
            <div class=style::cards style=move || format!("--most: {}", most.get())>
                <For
                    each=move || devices.get()
                    key=|device| device.id.clone()
                    children=move |device| view! { <Card device menu renaming /> }
                />
            </div>
        </Show>
    }
}
