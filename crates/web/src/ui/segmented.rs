use leptos::prelude::*;
use stylance::classes;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "segmented.module.scss"
);

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Segmented<T>(
    label: &'static str,
    choices: Vec<(T, &'static str)>,
    #[prop(into)] chosen: Signal<T>,
    #[prop(into)] on_choose: Callback<T>,
    #[prop(optional, into)] greyed: Option<Callback<T, bool>>,
    #[prop(optional)] class: &'static str,
    #[prop(optional, into)] hidden: Signal<bool>,
) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    view! {
        <div
            class=move || classes!(style::choice, hidden.get().then_some(style::hidden), class)
            role="radiogroup"
            aria-label=label
        >
            {choices
                .into_iter()
                .map(|(value, name)| {
                    view! {
                        <button
                            type="button"
                            role="radio"
                            aria-checked=move || (chosen.get() == value).to_string()
                            disabled=move || greyed.is_some_and(|greyed| greyed.run(value))
                            on:click=move |_| on_choose.run(value)
                        >
                            {name}
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}
