use leptos::prelude::*;
use stylance::classes;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "chips.module.scss"
);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Chip<T> {
    pub value: T,
    pub label: String,
    pub unable: Option<String>,
}

impl<T> Chip<T> {
    pub(crate) fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            unable: None,
        }
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Chips<T>(
    heading: &'static str,
    #[prop(into)] chips: Signal<Vec<Chip<T>>>,
    #[prop(into)] chosen: Signal<Option<T>>,
    #[prop(into)] on_choose: Callback<T>,
    #[prop(optional)] class: &'static str,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    view! {
        <span class=style::heading>{heading}</span>
        <div class=classes!(style::chips, class) role="radiogroup" aria-label=heading>
            {move || {
                let chosen = chosen.get();

                chips
                    .get()
                    .into_iter()
                    .map(|chip| {
                        let on = chosen.as_ref() == Some(&chip.value);
                        let value = chip.value;

                        view! {
                            <button
                                type="button"
                                role="radio"
                                aria-checked=on.to_string()
                                title=chip.unable.clone()
                                disabled=chip.unable.is_some()
                                on:click=move |_| on_choose.run(value.clone())
                            >
                                {chip.label}
                            </button>
                        }
                    })
                    .collect_view()
            }}
        </div>
    }
}
