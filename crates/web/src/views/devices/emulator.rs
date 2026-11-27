use leptos::prelude::*;
use protocol::{InstanceId, Topic};

use crate::app::{Global, Page};
use crate::ui;
use crate::ui::loadable::{Status, Wait};
use crate::ui::pane::{Crumb, Pane};
use crate::views::settings::{EmulatorForm, options};

#[allow(unreachable_pub)]
#[component]
pub(super) fn Emulator(avd: String, instance: InstanceId) -> impl IntoView {
    let Global { link, router, .. } = expect_context::<Global>();
    let sdk = options(move || instance.clone());
    let avd = StoredValue::new(avd);
    let name = Signal::derive(move || {
        avd.with_value(|avd| {
            link.emulator(avd)
                .map_or_else(|| avd.clone(), |device| device.name)
        })
    });
    let kept = Memo::new(move |_| avd.with_value(|avd| link.settings(avd)));
    let known = Memo::new(move |_| kept.with(Option::is_some));
    let back = Callback::new(move |()| router.go(Page::devices()));

    link.follow(move || Some(Topic::Emulator(avd.get_value())));

    view! {
        <Pane title=name crumbs=Crumb { name: "Devices", back }>
            <div class=ui::style::pane_body>
                <Status of=sdk loading=Wait::Blank let:android>
                    {move || {
                        // The form is built once the settings have come, and
                        // not again each time one of them changes.
                        let first = known.get().then(|| kept.get_untracked()).flatten()?;
                        let settings = Signal::derive(move || kept.get().unwrap_or_else(|| first.clone()));

                        Some(view! { <EmulatorForm avd=avd.get_value() android=android.clone() settings /> })
                    }}
                </Status>
            </div>
        </Pane>
    }
}
