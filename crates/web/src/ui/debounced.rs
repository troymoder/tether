use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use leptos::prelude::*;

use super::loadable::Loadable;
use crate::util::sleep;

/// Gives None while `sought` does, `Loading` from the first change until the
/// answer, and never the answer to something sought earlier. `fetch` runs
/// after the wait, so nothing it reads is tracked.
pub(crate) fn use_debounced<S, T, Fut>(
    sought: Signal<Option<S>>,
    wait: Duration,
    fetch: impl Fn(S) -> Fut + 'static,
) -> Signal<Option<Loadable<T>>>
where
    S: Clone + PartialEq + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    let fetch = Rc::new(fetch);
    let asked = LocalResource::new(move || {
        let wanted = sought.get();
        let fetch = Rc::clone(&fetch);

        async move {
            let wanted = wanted?;

            sleep(wait).await;

            // Typing went on meanwhile, and its own run asks.
            if sought.try_get_untracked().flatten().as_ref() != Some(&wanted) {
                return None;
            }

            let answer = fetch(wanted.clone()).await;

            Some((wanted, answer))
        }
    });

    Signal::derive(move || {
        let wanted = sought.get()?;

        Some(match asked.get().flatten() {
            Some((of, answer)) if of == wanted => answer.into(),
            _ => Loadable::Loading,
        })
    })
}
