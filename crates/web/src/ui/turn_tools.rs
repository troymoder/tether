//! The buttons go in a view's own row of tools, whose stylesheet lays them
//! out.

use leptos::prelude::*;

use crate::app::{Icon, icons};
use crate::ui::button;

fn quarter(turn: u16, left: bool) -> u16 {
    (turn + if left { 270 } else { 90 }) % 360
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn TurnTools(
    /// In degrees clockwise: 0, 90, 180 or 270.
    #[prop(into)]
    turn: Signal<u16>,
    #[prop(into)] on_turn: Callback<u16>,
    #[prop(optional, into)] disabled: Signal<bool>,
) -> impl IntoView {
    let tool = move |title: &'static str, label: &'static str, paths: &'static str, left: bool| {
        view! {
            <button
                type="button"
                class=button::style::icon
                title=title
                aria-label=label
                disabled=move || disabled.get()
                on:click=move |_| on_turn.run(quarter(turn.get_untracked(), left))
            >
                <Icon paths />
            </button>
        }
    };

    view! {
        {tool("Turn left", "Turn the picture a quarter left", icons::TURN_LEFT, true)}
        <code>{move || format!("{}°", turn.get())}</code>
        {tool("Turn right", "Turn the picture a quarter right", icons::TURN_RIGHT, false)}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_turn_wraps_round() {
        assert_eq!(quarter(0, true), 270);
        assert_eq!(quarter(270, false), 0);
        assert_eq!(quarter(90, true), 0);
        assert_eq!(quarter(90, false), 180);
    }
}
