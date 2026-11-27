//! A media channel is a DataChannel of its own for a job that moves a lot of
//! data, so the control channel never waits behind it. The first message says
//! what the channel is for. Dropping a lane closes it, which ends the job.

use std::cell::RefCell;
use std::rc::Rc;

use futures_channel::mpsc;
use futures_util::StreamExt as _;
use protocol::{Gather, Media, encode};
use web_sys::{RtcDataChannel, RtcDataChannelState, RtcDataChannelType};

use super::Link;
use super::heard::{Heard, bytes, ends};

pub(crate) struct Lane {
    channel: RtcDataChannel,
    /// The parts sent before the channel opened, which go out once it opens.
    /// None after that.
    early: Rc<RefCell<Option<Vec<Vec<u8>>>>>,
    came: RefCell<mpsc::UnboundedReceiver<Vec<u8>>>,
    _heard: Heard,
}

impl Link {
    pub(crate) fn lane(self, media: &Media) -> Option<Lane> {
        let pc = self.peer()?;
        let channel = pc.create_data_channel(protocol::MEDIA);

        channel.set_binary_type(RtcDataChannelType::Arraybuffer);

        let early: Rc<RefCell<Option<Vec<Vec<u8>>>>> = Rc::new(RefCell::new(Some(Vec::new())));
        let (to, came) = mpsc::unbounded();
        let to = Rc::new(RefCell::new(Some(to)));
        let gather = RefCell::new(Gather::default());
        let mut heard = Heard::new(&channel);
        let opened = channel.clone();
        let waiting = early.clone();

        heard.on("open", move |_| {
            for part in waiting.take().into_iter().flatten() {
                let _ = opened.send_with_u8_array(&part);
            }
        });

        let gone = to.clone();
        let ending = channel.clone();

        heard.on("message", move |event| {
            // tetherd leaves closing the DataChannel to the page.
            if ends(&event) {
                ending.close();
                drop(gone.take());

                return;
            }

            let Some(part) = bytes(&event) else {
                return;
            };

            if let Some(whole) = gather.borrow_mut().add(&part)
                && let Some(to) = &*gone.borrow()
            {
                let _ = to.unbounded_send(whole);
            }
        });

        // Dropping the sender ends the stream for whoever waits on it.
        heard.on("close", move |_| drop(to.take()));

        let lane = Lane {
            channel,
            early,
            came: RefCell::new(came),
            _heard: heard,
        };

        lane.send(&encode(media));

        Some(lane)
    }
}

impl Lane {
    /// Returns false once the channel has closed.
    pub(crate) fn send(&self, message: &[u8]) -> bool {
        let parts: Vec<Vec<u8>> = protocol::parts(message).collect();

        if let Some(early) = &mut *self.early.borrow_mut() {
            early.extend(parts);

            return true;
        }

        if self.channel.ready_state() != RtcDataChannelState::Open {
            return false;
        }

        parts
            .iter()
            .all(|part| self.channel.send_with_u8_array(part).is_ok())
    }

    /// Waiting for a message does not hold up sending on the lane meanwhile.
    pub(crate) async fn next(&self) -> Option<Vec<u8>> {
        std::future::poll_fn(|context| self.came.borrow_mut().poll_next_unpin(context)).await
    }

    /// The stream of messages received after this call. Holding it does not
    /// keep the channel open.
    pub(crate) fn heard(&self) -> mpsc::UnboundedReceiver<Vec<u8>> {
        std::mem::replace(&mut *self.came.borrow_mut(), mpsc::unbounded().1)
    }
}

impl Drop for Lane {
    fn drop(&mut self) {
        self.channel.close();
    }
}
