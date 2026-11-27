use leptos::prelude::*;

use super::panes::{Panes, Searched};
use super::reading::Reading;
use crate::ui::find::Find;

/// The first search goes to the latest hit. The log pauses there.
fn find(panes: Panes, reading: Reading, sought: &str, backwards: bool) {
    let (log, frame) = panes.shown();
    let text = sought.to_lowercase();

    if text.is_empty() {
        panes.search(None);
        panes.draw();
        return;
    }

    let current = match panes.searched.get_untracked() {
        Some(Searched::At { number, .. }) => Some(number),
        Some(Searched::Missed) | None => None,
    };
    // The row of the hit among the shown lines, its line number, and its
    // place among the hits.
    let hit = log.with_value(|log| {
        let matches: Vec<usize> = log
            .shown
            .iter()
            .enumerate()
            .filter(|(_, number)| {
                let line = log.parse(log.line(**number).unwrap_or_default());

                line.message.to_lowercase().contains(&text)
                    || line
                        .tag
                        .is_some_and(|tag| tag.to_lowercase().contains(&text))
            })
            .map(|(row, _)| row)
            .collect();
        let now = current.and_then(|number| log.shown.binary_search(&number).ok());
        let last = matches.len().saturating_sub(1);
        let at = match now {
            Some(now) if backwards => matches.iter().rposition(|row| *row < now).unwrap_or(last),
            Some(now) => matches.iter().position(|row| *row > now).unwrap_or(0),
            None => last,
        };
        let row = *matches.get(at)?;
        let number = *log.shown.get(row)?;

        Some((row, number, at + 1, matches.len()))
    });

    let Some((row, number, at, of)) = hit else {
        panes.search(Some(Searched::Missed));
        panes.draw();
        return;
    };

    panes.search(Some(Searched::At { number, at, of }));
    reading.pause();
    reading.unpin();

    // Centred, so the lines before it are in view too.
    if let Some(scroller) = frame.scroller.get_untracked() {
        let middle = frame.top_of(row) - f64::from(scroller.client_height()) / 2.0;

        scroller.set_scroll_top(middle.max(0.0));
    }

    panes.draw();
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn FindBar(
    panes: Panes,
    reading: Reading,
    /// The text in the bar's box, which the view keeps while the bar is
    /// closed.
    sought: RwSignal<String>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let count = Signal::derive(move || {
        panes.searched.get().map(|searched| match searched {
            Searched::At { at, of, .. } => format!("{at} of {of}"),
            Searched::Missed => "None".to_owned(),
        })
    });

    view! {
        <Find
            label="Find in the log"
            sought
            count
            find=move |backwards| find(panes, reading, &sought.get_untracked(), backwards)
            close=on_close
        />
    }
}
