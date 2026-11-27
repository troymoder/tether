use leptos::prelude::*;
use stylance::classes;

use crate::ui;

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Tabs<T>(
    tabs: Vec<(T, &'static str)>,
    #[prop(into)] open: Signal<T>,
    #[prop(into)] on_open: Callback<T>,
    /// The number waiting behind each tab. It is run tracked, so it may read
    /// signals.
    #[prop(optional, into)]
    counts: Option<Callback<T, usize>>,
) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    view! {
        <div class=classes!(ui::style::tabs, ui::style::under) role="tablist">
            {tabs
                .into_iter()
                .map(|(tab, name)| {
                    view! {
                        <button
                            type="button"
                            role="tab"
                            class=(ui::style::active, move || open.get() == tab)
                            aria-selected=move || (open.get() == tab).to_string()
                            on:click=move |_| on_open.run(tab)
                        >
                            {name}
                            {move || {
                                let count = counts.map_or(0, |counts| counts.run(tab));

                                (count > 0).then(|| view! { <span class=ui::style::count>{count}</span> })
                            }}
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}
