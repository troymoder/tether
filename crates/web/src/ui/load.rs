use std::future::Future;
use std::sync::Arc;

use leptos::prelude::*;

use super::loadable::Loadable;

/// `Loading` from the moment the key changes, and an answer for an older key
/// is dropped. To ask again for the same key, track a `Trigger` in `key` and
/// notify it. The loaded value stays shown until the new answer is here.
pub(crate) fn use_load<K, T, Fut>(
    key: impl Fn() -> K + Send + Sync + 'static,
    fetch: impl Fn(K) -> Fut + 'static,
) -> Signal<Loadable<T>>
where
    K: Clone + PartialEq + 'static,
    T: Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    Signal::derive(loaded(key, fetch))
}

/// For a value that is not `Send`, such as a JavaScript object.
pub(crate) fn use_load_local<K, T, Fut>(
    key: impl Fn() -> K + Send + Sync + 'static,
    fetch: impl Fn(K) -> Fut + 'static,
) -> Signal<Loadable<T>, LocalStorage>
where
    K: Clone + PartialEq + 'static,
    T: Clone + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    Signal::derive_local(loaded(key, fetch))
}

fn loaded<K, T, Fut>(
    key: impl Fn() -> K + Send + Sync + 'static,
    fetch: impl Fn(K) -> Fut + 'static,
) -> impl Fn() -> Loadable<T> + Send + Sync + 'static
where
    K: Clone + PartialEq + 'static,
    T: Clone + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    let key = Arc::new(key);
    let asked = LocalResource::new({
        let key = Arc::clone(&key);

        move || {
            let of = key();
            let asked = fetch(of.clone());

            async move { (of, asked.await) }
        }
    });

    move || {
        let now = key();

        match asked.get() {
            Some((of, answer)) if of == now => answer.into(),
            _ => Loadable::Loading,
        }
    }
}
