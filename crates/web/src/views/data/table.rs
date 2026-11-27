use leptos::prelude::*;

use super::style;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Table(rows: Vec<Vec<String>>) -> impl IntoView {
    let mut rows = rows.into_iter();
    let names = rows.next().unwrap_or_default();

    view! {
        <table>
            <thead>
                <tr>{names.into_iter().map(|name| view! { <th>{name}</th> }).collect_view()}</tr>
            </thead>
            <tbody>
                {rows
                    .map(|row| {
                        view! {
                            <tr>
                                {row
                                    .into_iter()
                                    .map(|cell| {
                                        let empty = cell == "NULL";

                                        view! { <td class=(style::null, empty)>{cell}</td> }
                                    })
                                    .collect_view()}
                            </tr>
                        }
                    })
                    .collect_view()}
            </tbody>
        </table>
    }
}
