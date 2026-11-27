//! The device drops its crashes once they are old, and the page never
//! deletes one. Clearing only hides the current ones, as a setting this
//! browser keeps (ui/kept.rs).

use android::crashes::{Crash, Kind};
use leptos::prelude::*;

use crate::app::Picked;
use crate::ui;
use crate::ui::dropdown::Choice;
use crate::ui::kept::kept_signal;
use crate::ui::load::use_load;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::pane::Pane;
use crate::ui::search::{Empty, matches};

mod device;
mod filters;
mod list;

pub(crate) use device::Crashes;
use filters::Filters;
use list::{CrashId, List};

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "crashes.module.scss"
);

fn cleared_key(device: &protocol::DeviceId) -> String {
    format!("crashes-cleared-{device}")
}

fn ready(crashes: &Loadable<Vec<Crash>>) -> &[Crash] {
    match crashes {
        Loadable::Ready(crashes) => crashes,
        Loadable::Loading | Loadable::Failed(_) => &[],
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Crashes() -> impl IntoView {
    let Picked { id, device, .. } = expect_context::<Picked>();
    let again = Trigger::new();
    let found = use_load(
        move || again.track(),
        move |()| {
            let device = device.get_untracked();

            async move { device.crashes().await }
        },
    );
    // The time the crashes were cleared up to. None shows them all.
    let cleared = kept_signal(cleared_key(&id), None::<String>);
    let sought = RwSignal::new(String::new());
    let kind = RwSignal::new(None::<Kind>);
    let open = RwSignal::new(None::<CrashId>);

    let since = Memo::new(move |_| {
        let Some(cleared) = cleared.get() else {
            return Loadable::Loading;
        };

        found.get().map(|crashes| {
            crashes
                .into_iter()
                .filter(|crash| cleared.as_ref().is_none_or(|cleared| crash.when > *cleared))
                .collect::<Vec<_>>()
        })
    });
    let listed = Memo::new(move |_| {
        let sought = sought.get();
        let kind = kind.get();

        since.get().map(|crashes| {
            crashes
                .into_iter()
                .filter(|crash| kind.is_none_or(|kind| crash.kind == kind))
                .filter(|crash| {
                    let process = crash.process.as_deref().unwrap_or_default();

                    matches(&sought, &[process, &crash.text])
                })
                .collect::<Vec<_>>()
        })
    });
    let kinds = Memo::new(move |_| {
        let mut kinds: Vec<Kind> =
            since.with(|since| ready(since).iter().map(|crash| crash.kind).collect());

        kinds.sort();
        kinds.dedup();

        std::iter::once(Choice::new(None, "Every kind"))
            .chain(
                kinds
                    .into_iter()
                    .map(|kind| Choice::new(Some(kind), kind.name())),
            )
            .collect::<Vec<_>>()
    });
    let counts = move || {
        let all = found.with(|found| found.as_ref().ready().map(Vec::len))?;
        let since = since.with(|since| since.as_ref().ready().map(Vec::len))?;

        Some((all, since))
    };
    let hidden = move || counts().map(|(all, since)| all - since);
    let latest = move || since.with(|since| ready(since).first().map(|crash| crash.when.clone()));

    view! {
        <Pane title="Crashes" class=style::view>
            <div class=ui::style::pane_body>
                <Filters
                    sought
                    kind
                    kinds
                    clearable=Signal::derive(move || latest().is_some())
                    on_clear=move |()| {
                        if let Some(latest) = untrack(latest) {
                            cleared.set(Some(latest));
                        }
                    }
                    on_again=move |()| again.notify()
                />
                <Status
                    of=found
                    empty=(Vec::is_empty, "Nothing has crashed on this device that it still knows of.")
                    let:_all
                >
                    <Status of=listed let:crashes>
                        <Empty
                            all=Signal::derive(move || counts().map_or(0, |(_, since)| since))
                            shown=crashes.len()
                            none="Nothing has crashed since they were cleared."
                        />
                        {(!crashes.is_empty()).then(|| view! { <List crashes open /> })}
                    </Status>
                </Status>
                {move || {
                    let hidden = hidden().filter(|hidden| *hidden != 0)?;

                    Some(
                        view! {
                            <p class=ui::style::hint>
                                {match hidden {
                                    1 => "1 that was cleared is hidden. ".to_owned(),
                                    hidden => format!("{hidden} that were cleared are hidden. "),
                                }}
                                <button type="button" class=style::crumb on:click=move |_| cleared.set(None)>
                                    "Show them again"
                                </button>
                            </p>
                        },
                    )
                }}
            </div>
        </Pane>
    }
}
