use leptos::prelude::*;
use stylance::classes;

use super::playing::Playing;
use super::stream::Stream;
use super::style;
use super::waiting::Waiting;
use crate::app::Picked;
use crate::ui::button::Button;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Over {
    Nothing,
    Disconnected,
    Reconnecting,
    Stopping,
    Booting,
}

fn over(how: Waiting, playing: Playing) -> Over {
    match (how, playing) {
        (_, Playing::Live { .. }) => Over::Nothing,
        // Only a device that should be connecting gets these notes.
        (Waiting::Connecting, Playing::GaveUp { .. }) => Over::Disconnected,
        (Waiting::Connecting, Playing::Retrying { .. }) => Over::Reconnecting,
        (Waiting::Connecting, Playing::Starting { tries, .. }) if tries > 0 => Over::Reconnecting,
        // The remaining notes need a picture to lie over. Without one, the
        // waiting surface shows the status instead.
        _ if playing.drawn().is_none() => Over::Nothing,
        (Waiting::Stopping, _) => Over::Stopping,
        (Waiting::Booting, _) => Over::Booting,
        _ => Over::Nothing,
    }
}

fn busy(what: &'static str) -> impl IntoView {
    view! {
        <p class=style::busy>{what}</p>
        <span class=style::bar aria-hidden="true"></span>
    }
}

fn chip(what: &'static str, bad: bool) -> impl IntoView {
    view! {
        <span class=style::chip class=(style::bad, bad)>
            {what}
        </span>
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Cover(
    resting: Memo<(Waiting, String)>,
    stream: Stream,
    #[prop(into)] shape: Signal<String>,
    #[prop(into)] drawn: Signal<bool>,
) -> impl IntoView {
    let control = expect_context::<Picked>().control;
    let how = move || resting.with(|(how, _)| *how);
    let lying = Memo::new(move |_| over(how(), control.playing.get()));

    // A shut down emulator shows an empty screen, like a phone that is off.
    let waiting = move || match how() {
        Waiting::NotConnected => view! { <p>"Not connected"</p> }.into_any(),
        Waiting::InUse => view! { <p>"In use elsewhere"</p> }.into_any(),
        Waiting::Off => ().into_any(),
        Waiting::Booting => busy("Booting…").into_any(),
        Waiting::Stopping => busy("Shutting down…").into_any(),
        // The cover shows the state instead.
        Waiting::Connecting if lying.get() != Over::Nothing => ().into_any(),
        Waiting::Connecting => busy("Connecting…").into_any(),
    };

    let cover = move || match lying.get() {
        Over::Nothing => ().into_any(),
        Over::Disconnected => {
            let name = resting.with(|(_, name)| name.clone());

            view! {
                <div class=style::cover role="status">
                    <p>"Disconnected"</p>
                    {chip("Disconnected", true)}
                    <p class=style::why>{format!("{name} stopped answering")}</p>
                    <Button small=true class=style::reconnect on:click=move |_| stream.reconnect()>
                        "Reconnect"
                    </Button>
                </div>
            }
            .into_any()
        }
        Over::Reconnecting => view! {
            <div class=style::cover role="status">
                {busy("Reconnecting…")}
                {chip("Reconnecting", false)}
            </div>
        }
        .into_any(),
        Over::Stopping => view! {
            <div class=style::cover role="status">
                {busy("Shutting down…")}
            </div>
        }
        .into_any(),
        Over::Booting => chip("Booting", false).into_any(),
    };

    view! {
        <Show when=move || !drawn.get()>
            <div
                class=classes!(style::surface, style::waiting)
                class=(style::off, move || how() == Waiting::Off)
                style=move || shape.get()
            >
                {waiting}
            </div>
        </Show>
        {cover}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAWN: Option<(u32, u32)> = Some((1080, 2400));

    #[test]
    fn a_live_video_has_nothing_over_it() {
        let live = Playing::Live {
            size: (1080, 2400),
            turned: None,
        };

        assert_eq!(over(Waiting::Connecting, live), Over::Nothing);
        assert_eq!(over(Waiting::Stopping, live), Over::Nothing);
    }

    #[test]
    fn retries_are_said_of_a_device_that_should_connect() {
        let retrying = Playing::Retrying {
            tries: 1,
            drawn: None,
        };
        let again = Playing::Starting {
            tries: 2,
            drawn: None,
            turned: None,
        };
        let first = Playing::Starting {
            tries: 0,
            drawn: DRAWN,
            turned: None,
        };
        let gave_up = Playing::GaveUp { drawn: DRAWN };

        assert_eq!(over(Waiting::Connecting, retrying), Over::Reconnecting);
        assert_eq!(over(Waiting::Connecting, again), Over::Reconnecting);
        assert_eq!(over(Waiting::Connecting, first), Over::Nothing);
        assert_eq!(over(Waiting::Connecting, gave_up), Over::Disconnected);
        assert_eq!(over(Waiting::NotConnected, gave_up), Over::Nothing);
    }

    #[test]
    fn an_emulator_that_stops_or_boots_says_so_over_its_picture() {
        let pictured = Playing::Pictures { drawn: DRAWN };
        let blank = Playing::Pictures { drawn: None };

        assert_eq!(over(Waiting::Stopping, pictured), Over::Stopping);
        assert_eq!(over(Waiting::Booting, pictured), Over::Booting);
        assert_eq!(over(Waiting::Stopping, blank), Over::Nothing);
    }
}
