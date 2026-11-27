use std::collections::HashMap;
use std::pin::pin;
use std::rc::Rc;
use std::time::Duration;

use futures_channel::oneshot;
use futures_util::StreamExt;
use futures_util::future::{join, select};
use leptos::prelude::*;

use super::device::{Process, Sources};
use super::log::{KEPT, Log};
use super::panes::{Panes, Whose};
use super::reading::Reading;
use super::sift::Sift;
use crate::app::{Global, Picked};
use crate::device::Device;
use crate::util::sleep;

#[derive(Clone, Copy)]
pub(super) struct DeviceLog {
    pub(super) failed: ReadSignal<Option<String>>,
    pub(super) running: ReadSignal<Vec<String>>,
    /// Lines come faster than they are worth drawing, so they wait here for
    /// the next tick.
    waiting: StoredValue<String, LocalStorage>,
    panes: Panes,
    sift: Sift,
    reading: Reading,
}

/// The log starts over when the view opens, when the device starts and when
/// the connection returns. The lines stay when the device stops, until it
/// answers again.
pub(super) fn use_device_log(panes: Panes, sift: Sift, reading: Reading) -> DeviceLog {
    let link = expect_context::<Global>().link;
    let from = expect_context::<Picked>().device;
    let failed = RwSignal::new(None::<String>);
    let running = RwSignal::new(Vec::<String>::new());
    let waiting = StoredValue::new_local(String::new());
    // Dropping it stops the reading of the device's log, as does the view
    // going away.
    let stop = StoredValue::new_local(None::<oneshot::Sender<()>>);
    // A memo, so the log only starts over when this changes and not each
    // time the device is listed anew.
    let following =
        Memo::new(move |_| link.connected.get() && from.get().sources().contains(Sources::DEVICE));

    // Ended processes stay, since their lines do.
    let known = move |listed: Vec<Process>| {
        sift.names.try_update_value(|names| {
            names.extend(
                listed
                    .into_iter()
                    .map(|process| (process.pid, process.name)),
            );
        });

        // The app choices: each app once, without the `:suffix` of its
        // other processes.
        let mut apps: Vec<String> = sift
            .names
            .try_with_value(|names| {
                names
                    .values()
                    .map(|name| name.split(':').next().unwrap_or(name).to_owned())
                    // An app is named like `com.example.app`. The rest are
                    // the system's, some in brackets.
                    .filter(|name| name.contains('.') && !name.starts_with('['))
                    .collect()
            })
            .unwrap_or_default();

        apps.sort();
        apps.dedup();

        if running
            .try_get_untracked()
            .is_some_and(|known| known != apps)
        {
            running.try_set(apps);
        }
    };

    let wait = move |lines: Vec<String>| {
        waiting.try_update_value(|waiting| {
            for line in lines {
                waiting.push_str(&line);
                waiting.push('\n');
            }
        });
    };

    // The device sends the lines it still has each time it is followed, so
    // what the view had of the log is dropped as they start to come.
    let start_over = move || {
        panes.device.update_value(Log::clear);
        waiting.update_value(String::clear);
        sift.names.update_value(HashMap::clear);
        running.set(Vec::new());
        panes.search(None);
        panes.picked.set(None);
        reading.resume();
    };

    // The process names come first so the app filter can place the lines the
    // device still has.
    let follow = move |followed: Rc<dyn Device>| {
        let (stopping, stopped) = oneshot::channel::<()>();

        stop.set_value(Some(stopping));

        leptos::task::spawn_local(async move {
            let whole = async {
                // Retried every 2 seconds, since a device that has just come
                // up may not answer yet.
                let mut lines = loop {
                    match followed.follow(KEPT).await {
                        Ok(lines) => break lines,
                        Err(error) => {
                            failed.try_set(Some(error));
                            sleep(Duration::from_secs(2)).await;
                        }
                    }
                };

                // A set that is handed back means the view has gone, and
                // its log with it.
                if failed.try_set(None).is_some() {
                    return;
                }

                start_over();

                if let Ok(listed) = followed.processes().await {
                    known(listed);
                }

                let read = async {
                    while let Some(new) = lines.next().await {
                        match new {
                            Ok(new) => wait(new),
                            Err(error) => {
                                failed.try_set(Some(error));
                                break;
                            }
                        }
                    }
                };
                let refreshed = async {
                    loop {
                        sleep(Duration::from_secs(3)).await;

                        if let Ok(listed) = followed.processes().await {
                            known(listed);
                        }
                    }
                };

                join(read, refreshed).await;
            };

            select(pin!(whole), stopped).await;

            // The error was this reading's, so it goes with it.
            failed.try_set(None);
        });
    };

    Effect::new(move |_| {
        stop.set_value(None);

        if following.get() {
            follow(from.get_untracked());
        }
    });

    DeviceLog {
        failed: failed.read_only(),
        running: running.read_only(),
        waiting,
        panes,
        sift,
        reading,
    }
}

impl DeviceLog {
    pub(super) fn drain(self) {
        let new = self
            .waiting
            .try_update_value(std::mem::take)
            .unwrap_or_default();

        if new.is_empty() {
            return;
        }

        if self.reading.is_paused_now() {
            self.panes.device.update_value(|log| {
                log.push(&new, None);
            });

            self.reading.held(new.lines().count());
        } else {
            self.sift.with(|filters| {
                self.panes.device.update_value(|log| {
                    log.push(&new, Some(filters));
                });
            });
        }

        self.reading.drawn_after(Whose::Device);
    }
}
