//! The labels are read in the background, a batch at a time, from the
//! browser's cache or else by the device's helper (`android::helper`). They
//! are kept in one place because the rows and the search both read them.

use std::collections::HashMap;
use std::rc::Rc;

use android::apps::Package;
use android::helper::Found;
use leptos::prelude::*;
use serde::Serialize;

use super::icon::{Icon, png_background};
use crate::app::Picked;
use crate::cache;
use crate::device::{Device, Helper};
use crate::ui::loadable::Loadable;

/// The most apps asked of the helper at once. One request draws every icon
/// of its batch before it answers, so a smaller batch shows its rows sooner.
const BATCH: usize = 48;

/// A new build is installed at a new path, and an update raises its
/// versionCode, so either misses the cache.
#[derive(Serialize)]
struct AppLabel<'a> {
    package: &'a str,
    version: u64,
    path: &'a str,
}

impl cache::Key for AppLabel<'_> {
    const KIND: &'static str = "app-label";
}

fn kept(package: &Package) -> AppLabel<'_> {
    AppLabel {
        package: &package.id,
        version: package.version,
        path: &package.path,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Label {
    pub(super) name: Option<String>,
    pub(super) version: Option<String>,
    pub(super) icon: Icon,
}

impl Label {
    fn of(found: Found) -> Self {
        let icon = match found.image {
            Ok(None) => Icon::None,
            Ok(Some(png)) => Icon::Drawn(png_background(&png)),
            Err(reason) => Icon::Failed(reason),
        };

        Label {
            name: found.label,
            version: found.version,
            icon,
        }
    }
}

type Answer = Result<Label, String>;

/// A package by its id, versionCode and path.
type Key = (String, u64, String);

fn key(package: &Package) -> Key {
    (package.id.clone(), package.version, package.path.clone())
}

type Asked = Signal<Rc<dyn Device>, LocalStorage>;

/// Started for the first batch the cache cannot answer, and stopped with
/// the view.
type Started = StoredValue<Option<Helper>, LocalStorage>;

#[derive(Clone, Copy)]
pub(super) struct Labels {
    /// A package is read once while the view is open, even when that failed.
    read: RwSignal<HashMap<Key, Answer>>,
}

impl Labels {
    pub(super) fn start(listed: Signal<Vec<Package>>) -> Self {
        let device = expect_context::<Picked>().device;
        let read = RwSignal::new(HashMap::new());
        let helper = StoredValue::new_local(None);
        let reading = StoredValue::new(false);
        // Told when a round of reading ends, so what was listed meanwhile
        // is looked at then.
        let finished = Trigger::new();

        // It starts a request and writes nothing itself. The answers come
        // back from the device later.
        Effect::new(move |_| {
            finished.track();

            let listed = listed.get();

            if reading.get_value() {
                return;
            }

            let missing: Vec<Package> = read.with_untracked(|read| {
                listed
                    .into_iter()
                    .filter(|package| !read.contains_key(&key(package)))
                    .collect()
            });

            if missing.is_empty() {
                return;
            }

            reading.set_value(true);

            leptos::task::spawn_local(async move {
                for batch in missing.chunks(BATCH) {
                    let answers = answered(device, helper, batch).await;

                    // Nothing is set once the view is gone.
                    if read.try_update(|read| read.extend(answers)).is_none() {
                        return;
                    }
                }

                if reading.try_set_value(false).is_none() {
                    finished.notify();
                }
            });
        });

        Self { read }
    }

    pub(super) fn of(self, package: &Package) -> Loadable<Label> {
        self.read.with(|read| match read.get(&key(package)) {
            Some(answer) => answer.clone().into(),
            None => Loadable::Loading,
        })
    }

    pub(super) fn name(self, package: &Package) -> Option<String> {
        self.of(package).ready()?.name
    }
}

async fn answered(device: Asked, helper: Started, batch: &[Package]) -> Vec<(Key, Answer)> {
    let mut answers = Vec::new();
    let mut missing = Vec::new();

    for package in batch {
        let cached = cache::read(&kept(package))
            .await
            .and_then(|bytes| Found::unkept(&bytes));

        match cached {
            Some(found) => answers.push((key(package), Ok(Label::of(found)))),
            None => missing.push(package),
        }
    }

    leptos::logging::log!(
        "app labels: batch of {}, {} from the cache, {} to ask",
        batch.len(),
        answers.len(),
        missing.len()
    );

    if missing.is_empty() {
        return answers;
    }

    let asking = async {
        let helper = started(device, helper).await?;
        let ids: Vec<&str> = missing.iter().map(|package| package.id.as_str()).collect();

        helper.apps(&ids).await
    };
    let mut read = match asking.await {
        Ok(read) => read,
        Err(error) => {
            leptos::logging::warn!("app labels: {error}");
            answers.extend(
                missing
                    .into_iter()
                    .map(|package| (key(package), Err(error.clone()))),
            );

            return answers;
        }
    };

    for package in missing {
        let at = read.iter().position(|one| one.id == package.id);
        let Some(found) = at.map(|at| read.swap_remove(at)) else {
            let unread = format!("label failed: {}: no answer", package.id);

            answers.push((key(package), Err(unread)));

            continue;
        };

        // One that failed is asked for again the next time the view opens,
        // since a newer helper may draw it. An app with no icon of its own
        // comes back without a reason, and is kept.
        match &found.image {
            Ok(_) => cache::write(&kept(package), &found.kept()).await,
            Err(reason) => leptos::logging::warn!("icons failed: {}: {reason}", found.id),
        }

        answers.push((key(package), Ok(Label::of(found))));
    }

    answers
}

async fn started(device: Asked, helper: Started) -> Result<Helper, String> {
    if let Some(helper) = helper.try_get_value().flatten() {
        return Ok(helper);
    }

    let Some(device) = device.try_get_untracked() else {
        return Err("label failed: the view is closed".to_owned());
    };

    let made = device.labels().await?;

    helper.try_set_value(Some(made.clone()));

    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(
        label: Option<&str>,
        version: Option<&str>,
        image: Result<Option<Vec<u8>>, String>,
    ) -> Found {
        Found {
            id: "com.example".to_owned(),
            label: label.map(str::to_owned),
            version: version.map(str::to_owned),
            image,
        }
    }

    #[test]
    fn no_picture_is_no_icon_and_a_reason_is_a_failure() {
        let bare = Label::of(found(None, None, Ok(None)));

        assert_eq!(bare.name, None);
        assert_eq!(bare.version, None);
        assert_eq!(bare.icon, Icon::None);

        let drawn = Label::of(found(Some("Maps"), Some("2.4.0"), Ok(Some(vec![1, 2, 3]))));

        assert_eq!(drawn.name.as_deref(), Some("Maps"));
        assert_eq!(drawn.version.as_deref(), Some("2.4.0"));
        assert!(matches!(drawn.icon, Icon::Drawn(_)));

        let failed = Label::of(found(Some("Maps"), None, Err("no icon".to_owned())));

        assert_eq!(failed.icon, Icon::Failed("no icon".to_owned()));
    }
}
