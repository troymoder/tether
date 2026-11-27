use leptos::prelude::*;
use stylance::classes;

use super::format::{sign, within};
use super::style;
use crate::app::{Icon, icons};

/// A match's path from the folder searched under, with true for a folder.
pub(super) type Match = (String, bool);

#[allow(unreachable_pub)]
#[component]
pub(super) fn SearchResults(
    folder: String,
    matches: Vec<Match>,
    /// Called with the whole path of a pressed match, and true for a folder.
    on_reach: Callback<(String, bool)>,
) -> impl IntoView {
    let empty = matches
        .is_empty()
        .then(|| view! { <p class=style::nothing>"No match. Enter goes to a path."</p> });
    let rows = matches
        .into_iter()
        .map(|(relative, is_folder)| {
            let at = within(&folder, &relative);
            let name = relative.rsplit('/').next().unwrap_or(&relative).to_owned();

            view! {
                <div class=classes!(style::file, style::item)>
                    <span></span>
                    <button
                        type="button"
                        class=classes!(style::name, is_folder.then_some(style::folder))
                        title=relative.clone()
                        on:click=move |_| on_reach.run((at.clone(), is_folder))
                    >
                        <Icon paths=if is_folder { icons::FOLDER } else { sign(&name) } />
                        <span>{relative.clone()}</span>
                    </button>
                    <code></code>
                    <code></code>
                    <span></span>
                </div>
            }
        })
        .collect_view();

    view! {
        {empty}
        {rows}
    }
}
