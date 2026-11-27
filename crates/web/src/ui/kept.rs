//! A kept setting lives in IndexedDB (app/storage.rs) and nowhere else. It is
//! read from there when the view opens and written there on each change.

use leptos::prelude::*;

use crate::app::{Read, forget, keep, kept};

pub(crate) trait Kept: Clone + 'static {
    /// None stores nothing, which forgets the key.
    fn text(&self) -> Option<String>;

    fn parse(text: &str) -> Option<Self>;
}

macro_rules! kept_as_text {
    ($($kind:ty),*) => {
        $(
            impl Kept for $kind {
                fn text(&self) -> Option<String> {
                    Some(self.to_string())
                }

                fn parse(text: &str) -> Option<Self> {
                    text.parse().ok()
                }
            }
        )*
    };
}

kept_as_text!(bool, u16, u32, f64, String);

impl<T: Kept> Kept for Option<T> {
    fn text(&self) -> Option<String> {
        self.as_ref().and_then(Kept::text)
    }

    fn parse(text: &str) -> Option<Self> {
        T::parse(text).map(Some)
    }
}

pub(crate) struct KeptSignal<T: 'static> {
    key: StoredValue<String>,
    read: LocalResource<T>,
}

impl<T> Clone for KeptSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for KeptSignal<T> {}

fn reading<T: Kept>(key: StoredValue<String>, default: T) -> LocalResource<T> {
    LocalResource::new(move || {
        let key = key.get_value();
        let default = default.clone();

        async move {
            let stored = kept(&key).await;

            stored
                .and_then(|stored| T::parse(&stored))
                .unwrap_or(default)
        }
    })
}

fn store<T: Kept>(key: &str, value: &T) {
    match value.text() {
        Some(text) => keep(key, &text),
        None => forget(key),
    }
}

pub(crate) fn kept_signal<T: Kept>(key: impl Into<String>, default: T) -> KeptSignal<T> {
    let key = StoredValue::new(key.into());

    KeptSignal {
        key,
        read: reading(key, default),
    }
}

impl<T: Kept> KeptSignal<T> {
    /// None until the browser's database has answered.
    pub(crate) fn get(self) -> Option<T> {
        self.read.get()
    }

    pub(crate) fn get_untracked(self) -> Option<T> {
        self.read.get_untracked()
    }

    pub(crate) fn set(self, value: T) {
        self.key.with_value(|key| store(key, &value));
        self.read.set(Some(value));
    }
}

/// A preference whose value is here, so that reading it gives no `Option`.
pub(crate) struct KeptHere<T: 'static> {
    key: StoredValue<String>,
    value: RwSignal<T>,
}

impl<T> Clone for KeptHere<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for KeptHere<T> {}

/// `key` has to be one of those app.rs reads into `start`.
pub(crate) fn kept_started<T>(start: &Read, key: &'static str, default: T) -> KeptHere<T>
where
    T: Kept + Send + Sync,
{
    let value = start.get(key).and_then(T::parse).unwrap_or(default);

    KeptHere {
        key: StoredValue::new(key.to_owned()),
        value: RwSignal::new(value),
    }
}

/// Nothing is drawn until the preference is read, so that the default never
/// flashes.
pub(crate) fn kept_then<T, V>(
    key: impl Into<String>,
    default: T,
    shown: impl Fn(KeptHere<T>) -> V + Send + 'static,
) -> impl IntoView
where
    T: Kept + Send + Sync,
    V: IntoView + 'static,
{
    let key = StoredValue::new(key.into());
    let read = reading(key, default);

    // Nothing sets `read`, so this builds once. Later changes go to the
    // signal made here.
    move || {
        read.get().map(|first| {
            shown(KeptHere {
                key,
                value: RwSignal::new(first),
            })
        })
    }
}

impl<T: Kept + Send + Sync> KeptHere<T> {
    pub(crate) fn get(self) -> T {
        self.value.get()
    }

    pub(crate) fn get_untracked(self) -> T {
        self.value.get_untracked()
    }

    pub(crate) fn read_only(self) -> ReadSignal<T> {
        self.value.read_only()
    }

    pub(crate) fn set(self, value: T) {
        self.key.with_value(|key| store(key, &value));
        self.value.set(value);
    }
}
