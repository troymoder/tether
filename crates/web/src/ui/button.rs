use leptos::prelude::*;
use stylance::classes;

use crate::app::Icon;

// Public so markup that only looks like a button, such as a popup's trigger,
// can take the same classes.
stylance::import_style!(#[allow(dead_code, unreachable_pub)] pub style, "button.module.scss");

/// A labelled button takes Blue and Red. An icon button takes Green and Red,
/// for start and stop.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum Tone {
    #[default]
    Plain,
    Blue,
    Green,
    Red,
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Button(
    #[prop(optional, into)] tone: Signal<Tone>,
    #[prop(optional)] quiet: bool,
    #[prop(optional)] small: bool,
    #[prop(optional)] submit: bool,
    #[prop(optional, into)] busy: Signal<bool>,
    /// Give this in place of `attr:disabled`, since `busy` sets the same
    /// attribute.
    #[prop(optional, into)]
    disabled: Signal<bool>,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    let tone = move || match tone.get() {
        Tone::Blue => Some(style::blue),
        Tone::Red => Some(style::red),
        Tone::Plain | Tone::Green => None,
    };

    view! {
        <button
            type=if submit { "submit" } else { "button" }
            class=move || {
                classes!(
                    style::button, tone(), quiet.then_some(style::quiet),
                    small.then_some(style::small), class
                )
            }
            disabled=move || busy.get() || disabled.get()
            aria-busy=move || busy.get().then_some("true")
        >
            {children()}
        </button>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn IconButton(
    #[prop(into)] paths: Signal<&'static str>,
    #[prop(into)] label: Signal<String>,
    #[prop(optional)] framed: bool,
    #[prop(optional)] small: bool,
    #[prop(optional, into)] lit: Signal<bool>,
    #[prop(optional, into)] tone: Signal<Tone>,
    #[prop(optional, into)] busy: Signal<bool>,
    /// Give this in place of `attr:disabled`, since `busy` sets the same
    /// attribute.
    #[prop(optional, into)]
    disabled: Signal<bool>,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    let tone = move || match tone.get() {
        Tone::Green => Some(style::start),
        Tone::Red => Some(style::stop),
        Tone::Plain | Tone::Blue => None,
    };

    view! {
        <button
            type="button"
            class=move || {
                classes!(
                    style::icon, framed.then_some(style::framed), small.then_some(style::small),
                    lit.get().then_some(style::lit), tone(), class
                )
            }
            title=label
            aria-label=label
            disabled=move || busy.get() || disabled.get()
            aria-busy=move || busy.get().then_some("true")
        >
            {move || view! { <Icon paths=paths.get() /> }}
        </button>
    }
}
