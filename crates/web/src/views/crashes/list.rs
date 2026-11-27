use android::crashes::{Crash, Kind};
use leptos::prelude::*;
use stylance::classes;

use super::style;
use crate::ui;
use crate::ui::tag::{Dot, Tag};

/// A device gives a crash no id, so it is told apart by its time, kind and
/// process.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CrashId {
    when: String,
    kind: Kind,
    process: Option<String>,
}

impl CrashId {
    fn of(crash: &Crash) -> Self {
        CrashId {
            when: crash.when.clone(),
            kind: crash.kind,
            process: crash.process.clone(),
        }
    }
}

/// `crashes` has at least one.
#[allow(unreachable_pub)]
#[component]
pub(super) fn List(
    crashes: Vec<Crash>,
    /// While it is None, the first crash is open, which is the latest. While
    /// it is not listed, none is.
    open: RwSignal<Option<CrashId>>,
) -> impl IntoView {
    let ids: Vec<CrashId> = crashes.iter().map(CrashId::of).collect();
    let texts: Vec<String> = crashes
        .iter()
        .map(|crash| {
            format!(
                "Process: {}\n\n{}",
                crash.process.as_deref().unwrap_or_default(),
                crash.text
            )
        })
        .collect();
    let listed = ids.clone();
    let front = Memo::new(move |_| {
        open.with(|open| match open {
            Some(open) => listed.iter().position(|id| id == open),
            None => Some(0),
        })
    });

    view! {
        <div class=classes!(
            ui::style::group, style::list
        )>
            {crashes
                .into_iter()
                .zip(ids)
                .enumerate()
                .map(|(index, (Crash { when, kind, process, .. }, id))| {
                    let tone = match kind {
                        Kind::NotResponding => Dot::Warn,
                        Kind::Crash | Kind::NativeCrash | Kind::Tombstone => Dot::Bad,
                    };

                    view! {
                        <button
                            type="button"
                            class=style::crash
                            class=(style::front, move || front.get() == Some(index))
                            on:click=move |_| open.set(Some(id.clone()))
                        >
                            <code class=style::when>{when}</code>
                            <Tag tone>{kind.name()}</Tag>
                            <code>{process.unwrap_or_else(|| "Unknown".to_owned())}</code>
                        </button>
                    }
                })
                .collect_view()}
        </div>
        <pre>{move || front.get().and_then(|front| texts.get(front).cloned())}</pre>
    }
}
