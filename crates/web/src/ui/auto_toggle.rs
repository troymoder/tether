use leptos::prelude::*;
use protocol::{DeviceId, Request};

use crate::app::Global;
use crate::link::ServerStoreFields;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "auto_toggle.module.scss"
);

#[allow(unreachable_pub)]
#[component]
pub(crate) fn AutoToggle(#[prop(into)] app: String, device: DeviceId) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let on = {
        let app = app.clone();
        let device = device.clone();

        Memo::new(move |_| {
            let app = link.server.apps().at_key(app.clone()).try_get();

            app.is_some_and(|app| app.auto_on(&device))
        })
    };

    view! {
        <label
            class=style::auto
            title="Auto-update: install new builds on this device as they arrive"
        >
            <input
                type="checkbox"
                aria-label="Auto-update"
                prop:checked=on
                on:change=move |event| {
                    link.ask(Request::AutoInstall {
                        id: app.clone(),
                        device: device.clone(),
                        on: event_target_checked(&event),
                    });
                }
            />
            <span class=style::words>"Auto-update"</span>
        </label>
    }
}
