//! A list that is empty because it has not loaded is `Loading`, never
//! `Ready(vec![])`. Having nothing is a property of the `Ready` value, which
//! `Status` says through its `empty` text.

use leptos::prelude::*;
use stylance::classes;

use crate::ui;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Loadable<T> {
    Loading,
    /// The reason, as the user is shown it.
    Failed(String),
    Ready(T),
}

impl<T> Loadable<T> {
    pub(crate) fn map<U>(self, change: impl FnOnce(T) -> U) -> Loadable<U> {
        match self {
            Loadable::Loading => Loadable::Loading,
            Loadable::Failed(why) => Loadable::Failed(why),
            Loadable::Ready(value) => Loadable::Ready(change(value)),
        }
    }

    pub(crate) fn as_ref(&self) -> Loadable<&T> {
        match self {
            Loadable::Loading => Loadable::Loading,
            Loadable::Failed(why) => Loadable::Failed(why.clone()),
            Loadable::Ready(value) => Loadable::Ready(value),
        }
    }

    pub(crate) fn ready(self) -> Option<T> {
        match self {
            Loadable::Ready(value) => Some(value),
            Loadable::Loading | Loadable::Failed(_) => None,
        }
    }

    /// An answer that failed keeps a value already read, since a device that
    /// can't answer for a moment still has what it had.
    pub(crate) fn after(last: Option<Self>, answer: Option<Result<T, String>>) -> Self {
        match (answer, last) {
            (Some(Ok(value)), _) => Loadable::Ready(value),
            (Some(Err(_)) | None, Some(Loadable::Ready(value))) => Loadable::Ready(value),
            (Some(Err(why)), _) => Loadable::Failed(why),
            (None, Some(last)) => last,
            (None, None) => Loadable::Loading,
        }
    }
}

impl<T> From<Result<T, String>> for Loadable<T> {
    fn from(answer: Result<T, String>) -> Self {
        match answer {
            Ok(value) => Loadable::Ready(value),
            Err(why) => Loadable::Failed(why),
        }
    }
}

/// The test for a ready value with nothing in it, and what is said then.
pub(crate) type Nothing<T> = (fn(&T) -> bool, &'static str);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wait {
    Say(&'static str),
    /// Nothing at all, for a value that comes at once and would only blink
    /// a line.
    Blank,
}

impl From<&'static str> for Wait {
    fn from(said: &'static str) -> Self {
        Wait::Say(said)
    }
}

/// `of` is a signal of either storage, so that a value that is not `Send`,
/// as `use_load_local` gives, is shown the same way.
#[allow(unreachable_pub)]
#[component]
pub(crate) fn Status<T, V, S>(
    #[prop(into)] of: Signal<Loadable<T>, S>,
    #[prop(optional)] empty: Option<Nothing<T>>,
    #[prop(into, default = Wait::Say("Looking…"))] loading: Wait,
    #[prop(optional)] class: &'static str,
    #[prop(optional)] failed: &'static str,
    /// Draws the line in place of the usual `<p>`. `class` and `failed` are
    /// not used then.
    #[prop(optional, into)]
    line: Option<Callback<String, AnyView>>,
    children: impl Fn(T) -> V + Send + Sync + 'static,
) -> impl IntoView
where
    T: Clone + 'static,
    V: IntoView + 'static,
    S: Storage<Loadable<T>>,
    Signal<Loadable<T>, S>: Get<Value = Loadable<T>>,
{
    let said = move |said: String, failed: Option<&'static str>| match line {
        Some(line) => line.run(said),
        None => {
            view! { <p class=classes!(ui::style::hint, ui::style::said, class, failed)>{said}</p> }
                .into_any()
        }
    };

    move || match of.get() {
        Loadable::Loading => match loading {
            Wait::Say(loading) => said(loading.to_owned(), None),
            Wait::Blank => ().into_any(),
        },
        Loadable::Failed(why) => said(why, Some(failed)),
        Loadable::Ready(value) => match empty {
            Some((nothing, text)) if nothing(&value) => said(text.to_owned(), None),
            _ => children(value).into_any(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_only_reaches_a_ready_value() {
        assert_eq!(Loadable::Ready(2).map(|n| n * 2), Loadable::Ready(4));
        assert_eq!(Loadable::<u8>::Loading.map(|n| n * 2), Loadable::Loading);
        assert_eq!(
            Loadable::<u8>::Failed("adb failed: closed".to_owned()).map(|n| n * 2),
            Loadable::Failed("adb failed: closed".to_owned())
        );
        assert_eq!(Loadable::Ready(2).ready(), Some(2));
        assert_eq!(Loadable::<u8>::Loading.ready(), None);
        assert_eq!(Loadable::from(Ok::<u8, String>(1)), Loadable::Ready(1));
    }

    #[test]
    fn a_failed_poll_keeps_the_last_reading() {
        let failed = || Some(Err("adb failed: closed".to_owned()));

        assert_eq!(Loadable::<u8>::after(None, None), Loadable::Loading);
        assert_eq!(Loadable::after(None, Some(Ok(1))), Loadable::Ready(1));
        assert_eq!(
            Loadable::<u8>::after(None, failed()),
            Loadable::Failed("adb failed: closed".to_owned())
        );
        assert_eq!(
            Loadable::after(Some(Loadable::Ready(1)), failed()),
            Loadable::Ready(1)
        );
        assert_eq!(
            Loadable::after(Some(Loadable::Ready(1)), Some(Ok(2))),
            Loadable::Ready(2)
        );
        assert_eq!(
            Loadable::after(Some(Loadable::Ready(1)), None),
            Loadable::Ready(1)
        );
    }
}
