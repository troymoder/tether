//! The view lists one connection, this browser's WebRTC connection to the
//! primary tetherd, which carries everything including the device's video.

use leptos::prelude::*;
use stylance::classes;

use crate::app::Global;
use crate::ui;
use crate::ui::pane::Pane;

mod detail;
mod figures;
mod row;

use detail::Detail;
use figures::lines;
use row::ConnectionRow;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "network.module.scss"
);

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Network() -> impl IntoView {
    let figures = expect_context::<Global>().link.figures;
    let open = RwSignal::new(false);
    let graph = Memo::new(move |_| {
        let traffic: Vec<_> = figures
            .traffic
            .with(|traffic| traffic.iter().copied().collect());
        let trips: Vec<_> = figures
            .round_trips
            .with(|trips| trips.iter().copied().collect());
        let losses: Vec<_> = figures
            .losses
            .with(|losses| losses.iter().copied().collect());

        lines(&traffic, &trips, &losses)
    });

    view! {
        <Pane title="Network" class=style::network>
            <div class=ui::style::pane_body>
                <div class=classes!(ui::style::group, style::sockets)>
                    <div class=classes!(style::row, style::head)>
                        <span>"From"</span>
                        <span>"To"</span>
                        <span>"Kind"</span>
                        <span>"State"</span>
                        <span class=style::figure>"Down"</span>
                        <span class=style::figure>"Up"</span>
                        <span>"Last minute"</span>
                    </div>
                    <div class=style::socket class=(style::open, move || open.get())>
                        <ConnectionRow open graph />
                        <Show when=move || open.get()>
                            <Detail graph />
                        </Show>
                    </div>
                </div>
                <span class=style::legend>
                    <span>
                        <span class=style::down aria-hidden="true"></span>
                        "Down"
                    </span>
                    <span>
                        <span class=style::up aria-hidden="true"></span>
                        "Up"
                    </span>
                    <span>
                        <span class=style::latency aria-hidden="true"></span>
                        "Latency"
                    </span>
                    <span>
                        <span class=style::loss aria-hidden="true"></span>
                        "Packet loss"
                    </span>
                </span>
            </div>
        </Pane>
    }
}
