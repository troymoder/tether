use leptos::prelude::*;

use super::super::style;
use crate::ui;
use crate::util::size;

#[allow(unreachable_pub)]
#[component]
pub(super) fn Progress(
    #[prop(optional)] what: Option<&'static str>,
    done: u64,
    total: u64,
) -> impl IntoView {
    let filled = if total == 0 {
        100.0
    } else {
        (done as f64 / total as f64 * 100.0).min(100.0)
    };
    let text = match what {
        Some(what) => format!("{what} · {}", of(done, total)),
        None => of(done, total),
    };

    view! {
        <div
            role="progressbar"
            aria-valuemin="0"
            aria-valuemax=total.to_string()
            aria-valuenow=done.to_string()
            aria-valuetext=text.clone()
        >
            <span class=style::meter style=format!("--filled: {filled:.1}%")></span>
            <span class=ui::style::hint>{text.clone()}</span>
        </div>
    }
}

/// Such as "3.1 of 8.7 MB", the unit said once when both have it.
fn of(done: u64, total: u64) -> String {
    let done = size(done);
    let total = size(total);

    match (done.rsplit_once(' '), total.rsplit_once(' ')) {
        (Some((number, unit)), Some((_, same))) if unit == same => format!("{number} of {total}"),
        _ => format!("{done} of {total}"),
    }
}

#[cfg(test)]
mod tests {
    use super::of;

    #[test]
    fn says_how_far() {
        assert_eq!(of(3_100_000, 8_700_000), "3.1 of 8.7 MB");
        assert_eq!(of(800_000, 8_700_000), "800.0 KB of 8.7 MB");
        assert_eq!(of(0, 0), "0 of 0 B");
    }
}
