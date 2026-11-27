use leptos::prelude::*;
use protocol::Platform;

use crate::app::icons;
use crate::ui;

pub(crate) fn platform_logo(platform: Platform) -> impl IntoView {
    let (system, logo) = match platform {
        Platform::Android => ("Android", icons::ANDROID),
        Platform::Ios => ("iOS", icons::APPLE),
    };

    view! {
        <svg
            class=ui::style::logo
            viewBox="0 0 24 24"
            role="img"
            aria-label=system
            inner_html=logo
        ></svg>
    }
}
