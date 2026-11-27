use leptos::prelude::*;
use stylance::classes;

use super::style;
use crate::app::icons;
use crate::ui;
use crate::ui::button::IconButton;
use crate::views::location::tiles::KINDS;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Controls(
    #[prop(into)] kind: Signal<usize>,
    #[prop(into)] on_kind: Callback<usize>,
    #[prop(into)] located: Signal<bool>,
    #[prop(into)] on_centre: Callback<()>,
    /// A number of zooms, in when it is above 0.
    #[prop(into)]
    on_zoom: Callback<f64>,
) -> impl IntoView {
    view! {
        <div
            class=classes!(style::over, style::kinds, ui::style::tabs)
            role="tablist"
            aria-label="The kind of map"
        >
            {KINDS
                .iter()
                .enumerate()
                .map(|(index, one)| {
                    view! {
                        <button
                            type="button"
                            role="tab"
                            class=(ui::style::active, move || kind.get() == index)
                            aria-selected=move || (kind.get() == index).to_string()
                            on:click=move |_| on_kind.run(index)
                        >
                            {one.name}
                        </button>
                    }
                })
                .collect_view()}
        </div>
        <div class=classes!(style::over, style::zoom)>
            <IconButton
                paths=icons::CENTRE
                label="Centre the map on the device"
                class=style::step
                disabled=Signal::derive(move || !located.get())
                on:click=move |_| on_centre.run(())
            />
            <IconButton
                paths=icons::ADD
                label="Zoom in"
                class=style::step
                on:click=move |_| on_zoom.run(1.0)
            />
            <IconButton
                paths=icons::LESS
                label="Zoom out"
                class=style::step
                on:click=move |_| on_zoom.run(-1.0)
            />
        </div>
        {move || {
            let kind = &KINDS[kind.get()];

            view! {
                <a
                    class=classes!(style::over, style::credit)
                    href=kind.more
                    target="_blank"
                    rel="noreferrer"
                >
                    {kind.credit}
                </a>
            }
        }}
    }
}
