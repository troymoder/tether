use leptos::prelude::*;
use stylance::classes;

use super::style;
use crate::ui::dropdown::Choice;

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Radios<T>(
    #[prop(into)] heading: String,
    #[prop(into)] choices: Signal<Vec<Choice<T>>>,
    #[prop(into)] chosen: Signal<Option<T>>,
    #[prop(into)] on_choose: Callback<T>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] side_by_side: bool,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    view! {
        <span class=style::heading>{heading.clone()}</span>
        <div
            class=classes!(style::radios, side_by_side.then_some(style::side_by_side))
            role="radiogroup"
            aria-label=heading
        >
            {move || {
                choices
                    .get()
                    .into_iter()
                    .map(|choice| {
                        let value = choice.value.clone();
                        let on = {
                            let value = value.clone();

                            move || chosen.with(|chosen| chosen.as_ref() == Some(&value))
                        };

                        view! {
                            <button
                                type="button"
                                role="radio"
                                aria-checked=move || on().to_string()
                                disabled=move || choice.disabled || disabled.get()
                                on:click=move |_| on_choose.run(value.clone())
                            >
                                <span class=style::mark aria-hidden="true"></span>
                                <span>{choice.label.clone()}</span>
                            </button>
                        }
                    })
                    .collect_view()
            }}
        </div>
    }
}
