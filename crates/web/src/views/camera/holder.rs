use leptos::prelude::*;
use protocol::{Feeder, Request};
use stylance::classes;

use super::fed::Cam;
use super::style;
use crate::app::{Global, Picked};
use crate::ui::button::Button;

/// `at` is in milliseconds since 1970.
fn clock(at: u64) -> String {
    let at = js_sys::Date::new(&(at as f64).into());

    format!("{:02}:{:02}", at.get_hours(), at.get_minutes())
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn HolderRow(
    cam: Cam,
    /// Always some while this shows.
    by: Memo<Option<Feeder>>,
    #[prop(into)] lost: Signal<bool>,
) -> impl IntoView {
    let Global { link, ui, .. } = expect_context::<Global>();
    let feed = expect_context::<Picked>().feed;
    let who = move || by.with(|by| by.as_ref().map(|by| by.name.clone()).unwrap_or_default());
    let print = move || by.with(|by| by.as_ref().map(|by| by.print.clone()).unwrap_or_default());
    // The line said of it, the question asked before taking it, and the verb.
    let words = move || {
        let who = who();
        let at = by.with(|by| by.as_ref().map_or_else(String::new, |by| clock(by.since)));

        if lost.get() {
            (
                format!("Taken over by {who} at {at}"),
                "Take the camera back?",
                "Take back",
            )
        } else {
            (
                format!("In use by {who} since {at}"),
                "Take over the camera?",
                "Take over",
            )
        }
    };
    let take = move |_| {
        let (_, title, verb) = untrack(words);
        let emulator = cam.name.get_value();

        ui.confirm_then(
            title,
            format!("{} stops feeding it.", untrack(who)),
            verb,
            async move {
                match link.call(Request::TakeCamera { emulator }).await {
                    Ok(_) => {
                        // Another device may have been picked while tetherd
                        // was asked.
                        feed.lost_camera.try_set(None);
                    }
                    Err(said) => link.say(said),
                }
            },
        );
    };

    view! {
        <div
            class=classes!(style::bar, style::held)
            class=(style::lost, move || lost.get())
            role="status"
        >
            <span class=style::mark aria-hidden="true"></span>
            <span class=style::holder title=print>
                {move || words().0}
            </span>
            <Button class=style::take on:click=take>
                {move || words().2}
            </Button>
        </div>
    }
}
