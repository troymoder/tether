use std::future::Future;
use std::time::Duration;

use leptos::prelude::*;

use super::loadable::Loadable;

/// A tick that comes while an answer is still awaited is skipped, so that
/// slow answers do not pile up. A failed answer keeps the last value. To ask
/// again at once, track a `Trigger` in `fetch` before its future and notify
/// it.
pub(crate) fn use_poll<T, Fut>(
    every: Duration,
    fetch: impl Fn() -> Fut + 'static,
) -> Signal<Loadable<T>>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    let asking = StoredValue::new(false);
    let tick = Trigger::new();
    let asked = LocalResource::new(move || {
        tick.track();
        asking.set_value(true);

        let asked = fetch();

        async move {
            let answer = asked.await;

            asking.try_set_value(false);

            answer
        }
    });
    let timer = set_interval_with_handle(
        move || {
            if !asking.get_value() {
                tick.notify();
            }
        },
        every,
    );

    if let Ok(timer) = timer {
        on_cleanup(move || timer.clear());
    }

    Memo::new(move |last: Option<&Loadable<T>>| Loadable::after(last.cloned(), asked.get())).into()
}
