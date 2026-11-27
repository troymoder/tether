use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use libadb::Session;
use libadb::channel::ChannelId;

use crate::broke;

pub struct Channel<S> {
    session: S,
    id: ChannelId,
    open: Arc<Open>,
}

struct Open {
    id: ChannelId,
    closed: AtomicBool,
    dropped: Arc<Mutex<Vec<ChannelId>>>,
}

impl Drop for Open {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::Relaxed) {
            self.dropped.lock().unwrap().push(self.id);
        }
    }
}

impl<S> Channel<S> {
    pub(crate) fn new(session: S, id: ChannelId, dropped: Arc<Mutex<Vec<ChannelId>>>) -> Self {
        Channel {
            session,
            id,
            open: Arc::new(Open {
                id,
                closed: false.into(),
                dropped,
            }),
        }
    }
}

impl<S> Channel<S>
where
    S: Session,
    S::IoError: std::fmt::Debug,
{
    pub async fn read(&mut self, into: &mut [u8]) -> Result<usize> {
        match self.session.read_channel(self.id, into).await {
            Err(libadb::Error::ChannelClosed) => Ok(0),
            read => read.map_err(broke),
        }
    }

    pub async fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.session
            .write_channel(self.id, bytes)
            .await
            .map_err(broke)
    }

    pub async fn close(mut self) -> Result<()> {
        self.open.closed.store(true, Ordering::Relaxed);
        self.session.close_channel(self.id).await.map_err(broke)
    }
}

impl<S: Clone> Channel<S> {
    pub fn handle(&self) -> Self {
        Channel {
            session: self.session.clone(),
            id: self.id,
            open: self.open.clone(),
        }
    }
}
