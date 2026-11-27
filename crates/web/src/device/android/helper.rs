//! A view starts the helper once and keeps it running while it holds a
//! [`Helper`]. Dropping the last handle stops it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adb::Said;
use android::helper::{self, Found, Kind, Medium, Record, Replies};
use futures_channel::{mpsc, oneshot};
use futures_util::StreamExt;
use leptos::task::spawn_local;

use crate::link::Link;
use crate::link::usb::Phone;

const FOLDERS: [&str; 3] = ["/sdcard/Pictures", "/sdcard/Movies", "/sdcard/DCIM/Camera"];

type Answer = oneshot::Sender<Result<Vec<Record>, String>>;

/// None once the helper has stopped.
type Waiting = Rc<RefCell<Option<HashMap<u32, Answer>>>>;

#[derive(Clone)]
pub(crate) struct Helper(Rc<Running>);

impl PartialEq for Helper {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

struct Running {
    requests: mpsc::UnboundedSender<Vec<u8>>,
    waiting: Waiting,
    next: Cell<u32>,
}

impl Helper {
    pub(crate) async fn start(link: Link, device: &adb::Device<Phone>) -> Result<Helper, String> {
        let dex = super::dex(link).await?;
        let adb::Process {
            mut input,
            mut output,
        } = helper::start(device, &dex)
            .await
            .map_err(|error| format!("helper failed: {error:#}"))?;
        let (requests, mut queued) = mpsc::unbounded::<Vec<u8>>();
        let waiting: Waiting = Rc::new(RefCell::new(Some(HashMap::new())));
        let answered = waiting.clone();

        // The queue ends when the last handle is dropped. Shutting the input
        // then stops the helper and ends the read below.
        spawn_local(async move {
            while let Some(request) = queued.next().await {
                if input.write(&request).await.is_err() {
                    break;
                }
            }

            let _ = input.shut().await;
        });

        spawn_local(async move {
            let mut replies = Replies::default();

            'read: while let Ok(Some(said)) = output.next().await {
                match said {
                    Said::Stdout(bytes) => replies.feed(&bytes),
                    Said::Stderr(bytes) => {
                        leptos::logging::warn!("helper: {}", String::from_utf8_lossy(&bytes));

                        continue;
                    }
                    Said::Exit(_) => break,
                }

                loop {
                    match replies.whole() {
                        Ok(Some(reply)) => {
                            let answer = answered
                                .borrow_mut()
                                .as_mut()
                                .and_then(|waiting| waiting.remove(&reply.id));

                            if let Some(answer) = answer {
                                let _ = answer.send(reply.records);
                            }
                        }
                        Ok(None) => break,
                        Err(error) => {
                            leptos::logging::warn!("{error:#}");

                            break 'read;
                        }
                    }
                }
            }

            // Dropping the answers fails every request still waiting.
            answered.borrow_mut().take();
        });

        Ok(Helper(Rc::new(Running {
            requests,
            waiting,
            next: Cell::new(0),
        })))
    }

    async fn ask(&self, kind: Kind, arguments: &[&str]) -> Result<Vec<Record>, String> {
        let stopped = || "helper stopped".to_owned();
        let id = self.0.next.get();
        let (answer, answered) = oneshot::channel();

        self.0.next.set(id.wrapping_add(1));
        self.0
            .waiting
            .borrow_mut()
            .as_mut()
            .ok_or_else(stopped)?
            .insert(id, answer);
        self.0
            .requests
            .unbounded_send(helper::request(id, kind, arguments))
            .map_err(|_| stopped())?;

        answered.await.map_err(|_| stopped())?
    }

    pub(crate) async fn apps(&self, ids: &[&str]) -> Result<Vec<Found>, String> {
        self.found(Kind::Apps, ids)
            .await
            .map_err(|error| format!("icons failed: {error}"))
    }

    pub(crate) async fn thumbnails(&self, paths: &[&str]) -> Result<Vec<Found>, String> {
        self.found(Kind::Thumbnails, paths)
            .await
            .map_err(|error| format!("thumbnails failed: {error}"))
    }

    async fn found(&self, kind: Kind, ids: &[&str]) -> Result<Vec<Found>, String> {
        self.ask(kind, ids)
            .await?
            .into_iter()
            .map(|record| Found::read(record).map_err(|error| format!("{error:#}")))
            .collect()
    }

    pub(crate) async fn count(&self) -> Result<u64, String> {
        let folders = folders();
        let arguments: Vec<&str> = folders.iter().map(String::as_str).collect();
        let records = self.ask(Kind::Count, &arguments).await;

        records
            .and_then(|records| helper::count(records).map_err(|error| format!("{error:#}")))
            .map_err(|error| format!("media failed: {error}"))
    }

    pub(crate) async fn page(&self, offset: usize, limit: usize) -> Result<Vec<Medium>, String> {
        let folders = folders();
        let offset = offset.to_string();
        let limit = limit.to_string();
        let arguments: Vec<&str> = [offset.as_str(), limit.as_str()]
            .into_iter()
            .chain(folders.iter().map(String::as_str))
            .collect();
        let records = self.ask(Kind::Page, &arguments).await;

        records
            .and_then(|records| {
                records
                    .into_iter()
                    .map(|record| Medium::read(record).map_err(|error| format!("{error:#}")))
                    .collect()
            })
            .map_err(|error| format!("media failed: {error}"))
    }

    /// A large read holds up every request behind it, so a stream or download
    /// uses a helper of its own.
    pub(crate) async fn read(
        &self,
        path: &str,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, String> {
        let offset = offset.to_string();
        let length = length.to_string();
        let records = self
            .ask(Kind::Read, &[path, offset.as_str(), length.as_str()])
            .await;

        records
            .and_then(|records| helper::slice(records).map_err(|error| format!("{error:#}")))
            .map_err(|error| format!("read failed: {error}"))
    }
}

fn folders() -> Vec<String> {
    FOLDERS.iter().map(|folder| helper::real(folder)).collect()
}
