use leptos::html;
use leptos::prelude::*;

use super::Global;
use super::icons;
use super::style;
use crate::ui::button::IconButton;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Toast() -> impl IntoView {
    let ui = expect_context::<Global>().ui;
    let toast = NodeRef::<html::Div>::new();
    let text = move || {
        ui.toast
            .with(|toast| toast.as_ref().map(|toast| toast.text.clone()))
    };

    // A popover shown later lies above earlier ones, so the toast is shown
    // again whenever it changes and whenever a popup opens, to stay above
    // the popups.
    Effect::new(move |_| {
        let said = ui.toast.with(Option::is_some);

        ui.adding.track();
        ui.confirm.track();

        if let Some(toast) = toast.get() {
            let _ = toast.hide_popover();

            if said {
                let _ = toast.show_popover();
            }
        }
    });

    view! {
        <div
            node_ref=toast
            popover="manual"
            role="status"
            class=style::notice
            class=(style::bad, move || text().is_some_and(|text| text.contains("failed")))
        >
            <span>{text}</span>
            <IconButton
                paths=icons::CLOSE
                label="Dismiss"
                small=true
                on:click=move |_| ui.toast.set(None)
            />
        </div>
    }
}
