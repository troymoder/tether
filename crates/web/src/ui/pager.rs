//! The view owns the page asked for and shows `Pages::clamped`, so a page
//! past the last, as after its items were deleted, is never shown and needs
//! no effect to put right.

use leptos::prelude::*;
use stylance::classes;

use crate::app::icons;
use crate::ui;
use crate::ui::button::IconButton;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "pager.module.scss"
);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pages {
    pub all: usize,
    pub each: usize,
}

impl Pages {
    pub(crate) fn count(self) -> usize {
        self.all.div_ceil(self.each)
    }

    /// Pages count from 0.
    pub(crate) fn clamped(self, page: usize) -> usize {
        page.min(self.count().saturating_sub(1))
    }

    /// The first and last item of a page, counted from 1. An empty list
    /// gives 0 and 0.
    pub(crate) fn range(self, page: usize) -> (usize, usize) {
        let first = (page * self.each + 1).min(self.all);
        let last = ((page + 1) * self.each).min(self.all);

        (first, last)
    }

    pub(crate) fn on_page(self, page: usize) -> usize {
        self.all.saturating_sub(page * self.each).min(self.each)
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Pager(
    #[prop(into)] page: Signal<usize>,
    /// None while the size of the list is not known. The pager then holds
    /// its place with nothing to press.
    #[prop(into)]
    pages: Signal<Option<Pages>>,
    #[prop(into)] on_turn: Callback<usize>,
) -> impl IntoView {
    let range = move || {
        let pages = pages.get()?;
        let (first, last) = pages.range(page.get());

        Some(format!("{first}–{last} of {}", pages.all))
    };

    let at_first = move || pages.with(Option::is_none) || page.get() == 0;
    let at_last = move || {
        pages
            .get()
            .is_none_or(|pages| pages.count() <= page.get() + 1)
    };

    view! {
        <div class=style::pager>
            <IconButton
                paths=icons::BACK
                label="Previous page"
                framed=true
                disabled=Signal::derive(at_first)
                on:click=move |_| on_turn.run(page.get_untracked().saturating_sub(1))
            />
            <span class=classes!(ui::style::hint, style::range)>{range}</span>
            <IconButton
                paths=icons::RIGHT
                label="Next page"
                framed=true
                disabled=Signal::derive(at_last)
                on:click=move |_| on_turn.run(page.get_untracked() + 1)
            />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_cover_the_list() {
        let pages = Pages { all: 130, each: 60 };

        assert_eq!(pages.count(), 3);
        assert_eq!(pages.range(0), (1, 60));
        assert_eq!(pages.range(2), (121, 130));
        assert_eq!(pages.on_page(1), 60);
        assert_eq!(pages.on_page(2), 10);
        assert_eq!(pages.clamped(1), 1);
        assert_eq!(pages.clamped(5), 2);
    }

    #[test]
    fn an_empty_list_has_no_pages() {
        let pages = Pages { all: 0, each: 60 };

        assert_eq!(pages.count(), 0);
        assert_eq!(pages.range(0), (0, 0));
        assert_eq!(pages.on_page(0), 0);
        assert_eq!(pages.clamped(3), 0);
    }
}
