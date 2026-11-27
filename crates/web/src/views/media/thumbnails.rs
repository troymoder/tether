use std::collections::HashMap;

use leptos::prelude::*;
use serde::Serialize;

use super::tile::{is_picture, is_video};
use super::{Address, Item};
use crate::app::Picked;
use crate::cache;
use crate::device::{Device, Helper};
use crate::ui::loadable::Loadable;

/// Pictures up to this many bytes are fetched whole when the helper made no
/// thumbnail.
const WHOLE: u64 = 1024 * 1024;

/// A batch shows only when all of it is done, and the helper makes a few a
/// second, so a small batch gets the first tiles on screen sooner.
const BATCH: usize = 12;

/// A changed file has a different size or time, so its old thumbnail misses
/// the cache.
#[derive(Serialize)]
pub(super) struct Thumbnail<'a> {
    path: &'a str,
    size: u64,
    changed: &'a str,
}

impl cache::Key for Thumbnail<'_> {
    const KIND: &'static str = "thumbnail";
}

pub(super) fn key(item: &Item) -> Thumbnail<'_> {
    Thumbnail {
        path: &item.path,
        size: item.size,
        changed: &item.changed,
    }
}

type Of = (String, String);

fn of(item: &Item) -> Of {
    (item.path.clone(), item.changed.clone())
}

fn pictured(item: &Item) -> bool {
    is_picture(item.name()) || is_video(item.name())
}

type Made = Result<Address, String>;

#[derive(Clone, Copy)]
pub(super) struct Thumbnails {
    made: RwSignal<HashMap<Of, Made>, LocalStorage>,
}

impl Thumbnails {
    pub(super) fn start(
        helper: StoredValue<Helper, LocalStorage>,
        page: Signal<Loadable<Vec<Item>>>,
    ) -> Self {
        let device = expect_context::<Picked>().device;
        let made = RwSignal::new_local(HashMap::new());
        // Counts the pages asked for, so the reads for a page that was left
        // stop.
        let asked = StoredValue::new(0u32);

        Effect::new(move |_| {
            let Loadable::Ready(items) = page.get() else {
                return;
            };

            let round = asked.get_value().wrapping_add(1);

            asked.set_value(round);

            leptos::task::spawn_local(async move {
                let on_page: Vec<Of> = items.iter().map(of).collect();
                let kept = made.try_update(|made: &mut HashMap<Of, Made>| {
                    made.retain(|item, _| on_page.contains(item));

                    made.keys().cloned().collect::<Vec<Of>>()
                });
                let Some(kept) = kept else {
                    return;
                };

                let missing: Vec<Item> = items
                    .into_iter()
                    .filter(|item| pictured(item) && !kept.contains(&of(item)))
                    .collect();

                for batch in missing.chunks(BATCH) {
                    let (Some(device), Some(helper)) =
                        (device.try_get_untracked(), helper.try_get_value())
                    else {
                        return;
                    };

                    let read = fetched(&*device, &helper, batch).await;

                    if asked.try_get_value() != Some(round)
                        || made.try_update(|made| made.extend(read)).is_none()
                    {
                        return;
                    }
                }
            });
        });

        Self { made }
    }

    pub(super) fn address(self, item: &Item) -> Option<String> {
        self.made.with(|made| {
            let made = made.get(&of(item))?.as_ref().ok()?;

            Some(made.as_str().to_owned())
        })
    }
}

async fn fetched(device: &dyn Device, helper: &Helper, batch: &[Item]) -> Vec<(Of, Made)> {
    let shown =
        |file: Vec<u8>| Address::of(&file).ok_or_else(|| "thumbnail failed: not shown".to_owned());
    let mut read = Vec::new();
    let mut missing = Vec::new();

    for item in batch {
        match cache::read(&key(item)).await {
            Some(kept) => read.push((of(item), shown(kept))),
            None => missing.push(item),
        }
    }

    if missing.is_empty() {
        return read;
    }

    let paths: Vec<&str> = missing.iter().map(|item| item.path.as_str()).collect();
    let asked = js_sys::Date::now();
    let found = helper.thumbnails(&paths).await;

    match &found {
        Ok(found) => {
            let failed = found.iter().filter(|found| found.image.is_err()).count();

            leptos::logging::log!(
                "thumbnails: helper gave {} of {} in {:.0} ms, {failed} failed",
                found.len(),
                paths.len(),
                js_sys::Date::now() - asked
            );

            for found in found {
                if let Err(reason) = &found.image {
                    leptos::logging::warn!("thumbnails failed: {}: {reason}", found.id);
                }
            }
        }
        Err(error) => leptos::logging::warn!("thumbnails: {error}"),
    }

    for item in missing {
        let made = match &found {
            Ok(found) => found
                .iter()
                .find(|found| found.id == item.path)
                .map_or(Ok(None), |found| found.image.clone())
                .and_then(|made| made.ok_or_else(|| "thumbnails failed: none made".to_owned())),
            Err(error) => Err(error.clone()),
        };
        let whole = is_picture(item.name()) && item.size <= WHOLE;
        let file = match made {
            Err(_) if whole => device.read(&item.path).await,
            made => made,
        };

        if let Ok(file) = &file {
            cache::write(&key(item), file).await;
        }

        read.push((of(item), file.and_then(shown)));
    }

    read
}
