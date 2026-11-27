use leptos::prelude::*;
use protocol::Event;

use super::style;
use crate::app::Global;

fn time(at: f64, full: bool) -> String {
    let then = js_sys::Date::new(&at.into());
    let now = js_sys::Date::new_0();
    let today = then.get_full_year() == now.get_full_year()
        && then.get_month() == now.get_month()
        && then.get_date() == now.get_date();
    let clock = format!(
        "{:02}:{:02}:{:02}",
        then.get_hours(),
        then.get_minutes(),
        then.get_seconds()
    );
    let month = then.get_month() + 1;
    let day = then.get_date();

    match (today, full) {
        (true, false) => clock,
        (true, true) => format!("today {clock}"),
        (false, false) => format!("{month:02}-{day:02} {}", &clock[..5]),
        (false, true) => format!("{}-{month:02}-{day:02} {clock}", then.get_full_year()),
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn EventRow(event: Event, open: RwSignal<Option<u64>>) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let Event {
        id,
        at,
        failed,
        what,
        message,
        device,
        place,
    } = event;
    let result = if failed { "failed" } else { "ok" };
    let opened = Memo::new(move |_| open.get() == Some(id));
    let message = Signal::derive(move || link.mentioned(&message));
    let device = Signal::derive(move || device.as_ref().map(|device| link.name(device)));
    let what = StoredValue::new(what);
    let place = StoredValue::new(place);
    // Opening the fields is the view's only layout shift, and it happens
    // where the user clicked.
    let whole = move || {
        let fields = [
            ("when", time(at as f64, true), None),
            ("result", result.to_owned(), Some(style::result)),
            ("what", what.get_value(), None),
            ("message", message.get(), None),
            (
                "device",
                device.get().unwrap_or_else(|| "none".to_owned()),
                None,
            ),
            ("where", place.get_value(), None),
        ];

        view! {
            <div class=style::whole class=(style::failed, failed)>
                {fields
                    .into_iter()
                    .map(|(name, value, class)| {
                        view! {
                            <span>{name}</span>
                            <span class=class>{value}</span>
                        }
                    })
                    .collect_view()}
            </div>
        }
    };

    view! {
        <button
            type="button"
            class=style::row
            class=(style::failed, failed)
            class=(style::on, move || opened.get())
            role="row"
            aria-expanded=move || opened.get().to_string()
            on:click=move |_| open.update(|open| *open = (*open != Some(id)).then_some(id))
        >
            <span>{time(at as f64, false)}</span>
            <span class=style::result>{result}</span>
            <span>{what.get_value()}</span>
            <span class=style::message>{message}</span>
            <span>{device}</span>
            <span>{place.get_value()}</span>
        </button>
        {move || opened.get().then(whole)}
    }
}
