//! The head is whatever the table has for one. A `<th>` of a real table pins
//! by itself, and a row of another kind takes `pinned_table::style::head`.

use leptos::prelude::*;
use stylance::classes;

// Public so a head row that is not a `<th>` can take the `head` class.
stylance::import_style!(#[allow(dead_code, unreachable_pub)] pub style, "pinned_table.module.scss");

#[allow(unreachable_pub)]
#[component]
pub(crate) fn PinnedTable(
    /// In pixels. A narrower box scrolls sideways.
    min_width: u16,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <div class=classes!(style::pinned, class)>
            <div style:min-width=format!("{min_width}px")>{children()}</div>
        </div>
    }
}
