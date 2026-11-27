use leptos::prelude::*;
use protocol::InstanceId;

use super::row::{Called, Selectable};
use super::{Key, Rows, style};
use crate::app::{Global, Icon, icons};
use crate::link::ServerStoreFields;
use crate::ui;
use crate::ui::button::Button;
use crate::ui::load::use_load;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::tag::{Dot, Tag};

#[allow(unreachable_pub)]
#[component]
pub(super) fn Instances(rows: Rows) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let listed = Memo::new(move |_| {
        link.server.access().with(|access| match access {
            Some(access) => Loadable::Ready(
                access
                    .instances
                    .iter()
                    .map(|instance| (instance.id.clone(), instance.id == access.primary))
                    .collect::<Vec<_>>(),
            ),
            None => Loadable::Loading,
        })
    });

    view! {
        <section>
            <h2>"Instances"</h2>
            <div class=ui::style::group>
                <Status of=listed let:instances>
                    // Keyed, since a row reused for another instance would
                    // keep that instance's Unlink.
                    <For each=move || instances.clone() key=Clone::clone let:instance>
                        {
                            let (id, primary) = instance;

                            if primary {
                                view! { <PrimaryRow id rows /> }.into_any()
                            } else {
                                view! { <LinkedRow id rows /> }.into_any()
                            }
                        }
                    </For>
                </Status>
            </div>
        </section>
    }
}

#[allow(unreachable_pub)]
#[component]
fn PrimaryRow(id: InstanceId, rows: Rows) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let key = Key::Instance(id.clone());
    let open = RwSignal::new(false);
    // The primary runs emulators when it has an Android SDK to make them
    // from. It is asked as the row is built, so the answer is there when
    // the row opens.
    let sdk = {
        let id = id.clone();

        use_load(
            || (),
            move |()| {
                let id = id.clone();

                async move {
                    let options = crate::link::capture::emulator_options(link, id).await;

                    Ok::<_, String>(options.is_ok())
                }
            },
        )
    };
    let name = {
        let id = id.clone();

        Signal::derive(move || link.instance_name(&id))
    };
    // This browser is connected to the primary directly, since it served
    // the page.
    let reached = move || {
        let (tone, class, how) = if link.connected.get() {
            (Dot::On, style::direct, "Direct")
        } else {
            (Dot::Grey, style::offline, "Offline")
        };

        view! {
            <Tag tone class>
                {how}
            </Tag>
        }
    };

    view! {
        <Selectable rows of=key.clone()>
            <span class=style::name>
                <span class=style::title>
                    <button
                        type="button"
                        class=style::toggle
                        aria-label="What it runs"
                        aria-expanded=move || open.get().to_string()
                        on:click=move |_| open.update(|open| *open = !*open)
                    >
                        {move || {
                            let paths = if open.get() { icons::CHEVRON } else { icons::RIGHT };

                            view! { <Icon paths /> }
                        }}
                    </button>
                    <Called rows of=key name />
                </span>
                <Tag>"Primary"</Tag>
                {reached}
            </span>
            // The primary keeps the button's room, so its row lines up with
            // the others.
            <Button quiet=true disabled=true attr:style="visibility: hidden">
                "Unlink"
            </Button>
            <code class=style::line>
                <span>{id.0}</span>
            </code>
            <Show when=move || open.get()>
                <Parts emulators=Signal::derive(move || sdk.get() == Loadable::Ready(true)) />
            </Show>
        </Selectable>
    }
}

#[allow(unreachable_pub)]
#[component]
fn Parts(emulators: Signal<bool>) -> impl IntoView {
    let part = |name: &'static str, about: &'static str| {
        view! {
            <div>
                <span>{name}</span>
                <span class=ui::style::hint>{about}</span>
            </div>
        }
    };

    view! {
        <div class=style::parts>
            <Show when=move || {
                emulators.get()
            }>{part("Android emulator", "Runs Android emulators.")}</Show>
            {part("adb", "Android phones over USB and Wi-Fi.")}
            {part("App registry", "Stores uploaded builds.")}
        </div>
    }
}

#[allow(unreachable_pub)]
#[component]
fn LinkedRow(id: InstanceId, rows: Rows) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let key = Key::Instance(id.clone());
    let unlinked = key.clone();
    let name = {
        let id = id.clone();

        Signal::derive(move || link.instance_name(&id))
    };

    view! {
        <Selectable rows of=key.clone()>
            <span class=style::name>
                <span class=style::title>
                    <Called rows of=key name />
                </span>
            </span>
            <Button quiet=true on:click=move |_| rows.remove.run(unlinked.clone())>
                "Unlink"
            </Button>
            <code class=style::line>
                <span>{id.0}</span>
            </code>
        </Selectable>
    }
}
