//! tetherd keeps the events and does the filtering and sorting, so each
//! change to what is asked sends it a new query and gets one page back.

use leptos::prelude::*;
use protocol::{DeviceId, EventPage, EventQuery};
use stylance::classes;

use crate::app::{Global, Icon, icons};
use crate::ui;
use crate::ui::button::Button;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::pane::Pane;
use crate::ui::pinned_table::{self, PinnedTable};
use crate::ui::search::SearchBox;

mod filter_list;
mod header_filter;
mod row;

use filter_list::FilterList;
use header_filter::HeaderFilter;
use row::EventRow;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "events.module.scss"
);

const PAGE: u32 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    Result,
    What,
    Device,
    Place,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Cuts {
    failed: Option<bool>,
    what: Option<String>,
    device: Option<DeviceId>,
    place: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Asked {
    search: Option<String>,
    cuts: Cuts,
    oldest_first: bool,
}

/// The number of events "Show more" raised the page to, and the query it
/// was pressed for. It counts for nothing once something else is asked, so
/// no effect has to reset it.
#[derive(Clone, Debug, PartialEq)]
struct More {
    of: Asked,
    most: u32,
}

fn named(values: &[String]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|value| (value.clone(), value.clone()))
        .collect()
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Events() -> impl IntoView {
    let link = expect_context::<Global>().link;
    let sought = RwSignal::new(String::new());
    let cuts = RwSignal::new(Cuts::default());
    let oldest_first = RwSignal::new(false);
    let more = RwSignal::new(None::<More>);
    let asked = Memo::new(move |_| {
        let search = sought.get().trim().to_owned();

        Asked {
            search: (!search.is_empty()).then_some(search),
            cuts: cuts.get(),
            oldest_first: oldest_first.get(),
        }
    });
    let most = move || {
        let asked = asked.get();

        more.with(|more| match more {
            Some(more) if more.of == asked => more.most,
            _ => PAGE,
        })
    };
    // Subscribed with the query, so tetherd sends the page again whenever
    // an event is kept, and again for each new query.
    let query = Memo::new(move |_| {
        let Asked {
            search,
            cuts,
            oldest_first,
        } = asked.get();

        EventQuery {
            search,
            failed: cuts.failed,
            what: cuts.what,
            device: cuts.device,
            place: cuts.place,
            oldest_first,
            most: most(),
        }
    });
    // The last page stays while a new query is asked, so the table does not
    // blink as the search is typed.
    let events = RwSignal::new(Loadable::<EventPage>::Loading);

    link.events(
        move || query.get(),
        Callback::new(move |page| events.set(Loadable::Ready(page))),
    );

    let listed =
        Memo::new(move |_| events.with(|page| page.as_ref().map(|page| page.events.clone())));
    let whats =
        Memo::new(move |_| events.with(|page| page.as_ref().map(|page| named(&page.whats))));
    let places =
        Memo::new(move |_| events.with(|page| page.as_ref().map(|page| named(&page.places))));
    let devices = Memo::new(move |_| {
        events.with(|page| {
            page.as_ref().map(|page| {
                page.devices
                    .iter()
                    .map(|id| (id.clone(), link.name(id)))
                    .collect::<Vec<_>>()
            })
        })
    });
    let results = Signal::derive(|| {
        Loadable::Ready(vec![(true, "failed".to_owned()), (false, "ok".to_owned())])
    });
    let more_match =
        move || events.with(|page| page.as_ref().ready().is_some_and(|page| page.more));
    let open = RwSignal::new(None::<u64>);
    let listing = RwSignal::new(None::<Column>);
    let failed_cut = Signal::derive(move || cuts.with(|cuts| cuts.failed));
    let what_cut = Signal::derive(move || cuts.with(|cuts| cuts.what.clone()));
    let device_cut = Signal::derive(move || cuts.with(|cuts| cuts.device.clone()));
    let place_cut = Signal::derive(move || cuts.with(|cuts| cuts.place.clone()));
    let cut_failed = move |failed: Option<bool>| cuts.update(|cuts| cuts.failed = failed);
    let cut_what = move |what: Option<String>| cuts.update(|cuts| cuts.what = what);
    let cut_device = move |device: Option<DeviceId>| cuts.update(|cuts| cuts.device = device);
    let cut_place = move |place: Option<String>| cuts.update(|cuts| cuts.place = place);
    let failed_name = Signal::derive(move || {
        let failed = failed_cut.get()?;

        Some(if failed { "failed" } else { "ok" }.to_owned())
    });
    let device_name = Signal::derive(move || Some(link.name(&device_cut.get()?)));

    view! {
        <Pane title="Events" class=style::events>
            <div class=ui::style::pane_body>
                <label class=style::search>
                    <Icon paths=icons::SEARCH />
                    <SearchBox sought label="Search the messages" placeholder="Search messages" />
                </label>
                // The lists of the headers hang over the scrolling box and
                // not in it, so it never clips them.
                <div class=style::table_frame>
                    <PinnedTable min_width=720 class=style::table_box>
                        <div class=style::table role="table">
                            <div
                                class=classes!(style::row, style::heads, pinned_table::style::head)
                                role="row"
                            >
                                <span class=style::head>
                                    <button
                                        title=move || {
                                            if oldest_first.get() {
                                                "Oldest first"
                                            } else {
                                                "Newest first"
                                            }
                                        }
                                        on:click=move |_| {
                                            oldest_first.update(|oldest| *oldest = !*oldest)
                                        }
                                    >
                                        <span>"Time"</span>
                                        {move || {
                                            let paths = if oldest_first.get() {
                                                icons::UP
                                            } else {
                                                icons::DOWN
                                            };

                                            view! { <Icon paths /> }
                                        }}
                                    </button>
                                </span>
                                <HeaderFilter
                                    title="Result"
                                    cut=failed_name
                                    column=Column::Result
                                    listing
                                />
                                <HeaderFilter
                                    title="What"
                                    cut=what_cut
                                    column=Column::What
                                    listing
                                />
                                <span class=classes!(style::head, style::plain)>"Message"</span>
                                <HeaderFilter
                                    title="Device"
                                    cut=device_name
                                    column=Column::Device
                                    listing
                                />
                                <HeaderFilter
                                    title="Where"
                                    cut=place_cut
                                    column=Column::Place
                                    listing
                                />
                            </div>
                            <Status of=listed empty=(Vec::is_empty, "Nothing.") let:events>
                                {events
                                    .into_iter()
                                    .map(|event| view! { <EventRow event open /> })
                                    .collect_view()}
                            </Status>
                        </div>
                    </PinnedTable>
                    <FilterList
                        label="Results"
                        column=Column::Result
                        listing
                        values=results
                        cut=failed_cut
                        on_cut=cut_failed
                        under=style::under_result
                    />
                    <FilterList
                        label="What was done"
                        column=Column::What
                        listing
                        values=whats
                        cut=what_cut
                        on_cut=cut_what
                        under=style::under_what
                    />
                    <FilterList
                        label="Devices"
                        column=Column::Device
                        listing
                        values=devices
                        cut=device_cut
                        on_cut=cut_device
                        under=style::under_device
                    />
                    <FilterList
                        label="Places"
                        column=Column::Place
                        listing
                        values=places
                        cut=place_cut
                        on_cut=cut_place
                        under=style::under_place
                    />
                </div>
                <Show when=more_match>
                    <div class=style::more>
                        <Button on:click=move |_| {
                            more.set(
                                Some(More {
                                    of: asked.get_untracked(),
                                    most: untrack(most) + PAGE,
                                }),
                            )
                        }>"Show more"</Button>
                    </div>
                </Show>
            </div>
        </Pane>
    }
}
