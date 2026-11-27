use leptos::prelude::*;
use leptos::{ev, html};
use protocol::{Condition, Device, DeviceId, Request};

use super::arrows::go;
use super::{delete, listed, starts, style};
use crate::app::{Global, Panel};
use crate::ui::logo::platform_logo;
use crate::ui::menu::MenuOf;
use crate::ui::renamable::{Renamable, Renaming};
use crate::ui::tag::Tag;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Card(
    device: Device,
    menu: MenuOf<DeviceId>,
    renaming: Renaming<DeviceId>,
) -> impl IntoView {
    let Global { link, router, ui } = expect_context::<Global>();
    let id = device.id.clone();
    let platform = id.platform();
    let serial = id.serial();
    let key = id.to_string();
    // The card is not made again when its device changes, so it reads the
    // device from the store. The last listing stays for the moment between
    // tetherd dropping the device and the list dropping the card.
    let device = Memo::new({
        let id = id.clone();

        move |last: Option<&Device>| {
            link.device(&id)
                .unwrap_or_else(|| last.unwrap_or(&device).clone())
        }
    });
    let id = StoredValue::new(id);
    let specs = Memo::new(move |_| {
        let avd = device.with(|device| Some(device.emulator.as_ref()?.avd.clone()))?;

        link.settings(&avd)
    });
    let new = Memo::new(move |_| device.with(listed::new));
    let picked = Memo::new(move |_| id.with_value(|id| router.device().as_ref() == Some(id)));
    let name = Memo::new(move |_| device.with(|device| device.name.clone()));
    let phone = move |condition: Condition| {
        device.with(|device| device.emulator.is_none() && device.condition == condition)
    };
    let about =
        move || device.with(|device| specs.with(|specs| listed::about(device, specs.as_ref())));
    let at = move || {
        device.with(|device| match &device.id {
            DeviceId::Emulator { instance, .. } => Some(link.instance_name(instance)),
            DeviceId::Android(_) | DeviceId::Iphone(_) => {
                let way = device
                    .connected()
                    .or_else(|| device.routes.first().map(|route| &route.way))
                    .or_else(|| device.last.as_ref().map(|last| &last.way))?;

                Some(link.place(&way.at))
            }
        })
    };
    let node = NodeRef::<html::Div>::new();

    let press = move |twice: bool| {
        let device = device.get_untracked();

        if listed::new(&device) {
            link.connect_to(ui, device.id, None);
            return;
        }

        let starts = starts(&device);

        router.pick(Some(device.id.clone()));

        if !twice {
            // On a page where the views cover the device, they move out of
            // its way.
            ui.panel.set(Panel::Auto);
        } else if starts {
            link.connect_to(ui, device.id, None);
        }
    };

    let keyed = move |event: ev::KeyboardEvent| {
        let device = device.get_untracked();
        let new = listed::new(&device);

        match event.key().as_str() {
            "Enter" | " " => press(false),
            key @ ("ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown") => go(router, key),
            "F2" if !new => renaming.start(device.id),
            "Delete" | "Backspace" if !new => delete(ui, link, &device),
            _ => return,
        }

        event.prevent_default();
    };

    // The focus goes back to the card when the rename ends with nothing
    // else focused, as after Enter or Escape.
    let done = move |()| {
        request_animation_frame(move || {
            let body: Option<web_sys::Element> = document().body().map(Into::into);
            let focused = document().active_element();

            if (focused.is_none() || focused == body)
                && let Some(node) = node.get_untracked()
            {
                let _ = node.focus();
            }
        });
    };

    view! {
        <div
            node_ref=node
            role="button"
            tabindex="0"
            data-card=key
            data-new=move || new.get().then_some("")
            class=style::card
            class=(style::picked, move || picked.get())
            class=(style::away, move || phone(Condition::Unavailable))
            class=(style::found, move || phone(Condition::Available))
            aria-pressed=move || (!new.get()).then(|| picked.get().to_string())
            title=move || device.with(listed::tooltip)
            // The browser counts the clicks.
            on:click=move |event| press(event.detail() == 2)
            on:keydown=keyed
            on:contextmenu=move |event| menu.at_pointer(&event, id.get_value())
        >
            <header>
                <span class=style::who>
                    {platform_logo(platform)}
                    <Renamable
                        renaming
                        of=id.get_value()
                        name
                        renamed=move |name| {
                            link.ask(Request::RenameDevice {
                                device: id.get_value(),
                                name,
                            });
                        }
                        done
                        class=style::renaming
                    >
                        <strong>{name}</strong>
                    </Renamable> <Show when=move || new.get()>
                        <Tag class=style::new>"New"</Tag>
                    </Show>
                </span>
                {move || {
                    let (tag, tone, _) = device.with(listed::tag);

                    view! { <Tag tone>{tag}</Tag> }
                }}
            </header>
            <span class=style::line>
                <code>{serial}</code>
                <code>{at}</code>
            </span>
            <p title=about>{about}</p>
        </div>
    }
}
