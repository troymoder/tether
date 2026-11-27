//! The page creates the channel and transfers it in the same task without
//! using it, because a data channel can only be transferred then. The worker
//! sends the channel's first message, which says what it is for, once it opens.

use std::cell::RefCell;

use js_sys::{ArrayBuffer, Object, Reflect, Uint8Array};
use protocol::{CameraFrame, Gather, encode};
use wasm_bindgen::prelude::*;
use web_sys::{
    DedicatedWorkerGlobalScope, MessageEvent, RtcDataChannel, RtcDataChannelState,
    RtcDataChannelType,
};

/// The number of bytes that may wait to be sent before frames are dropped.
const WAITING: u32 = 8 * 1024 * 1024;

pub(super) struct Sink {
    channel: RtcDataChannel,
    _open: Closure<dyn Fn()>,
    _message: Closure<dyn Fn(MessageEvent)>,
}

impl Sink {
    /// `first` is sent once the channel opens, and tetherd's messages on it go
    /// to the page as `{ held }`.
    pub(super) fn new(
        channel: RtcDataChannel,
        first: Vec<u8>,
        page: &DedicatedWorkerGlobalScope,
    ) -> Self {
        channel.set_binary_type(RtcDataChannelType::Arraybuffer);

        let opened = channel.clone();
        let open = Closure::<dyn Fn()>::new(move || {
            for part in protocol::parts(&first) {
                let _ = opened.send_with_u8_array(&part);
            }
        });
        let gather = RefCell::new(Gather::default());
        let page = page.clone();
        let message = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
            let Ok(buffer) = event.data().dyn_into::<ArrayBuffer>() else {
                return;
            };

            let part = Uint8Array::new(&buffer).to_vec();

            if let Some(whole) = gather.borrow_mut().add(&part) {
                let told = Object::new();
                let _ = Reflect::set(&told, &"held".into(), &Uint8Array::from(whole.as_slice()));
                let _ = page.post_message(&told);
            }
        });

        channel.set_onopen(Some(open.as_ref().unchecked_ref()));
        channel.set_onmessage(Some(message.as_ref().unchecked_ref()));

        Self {
            channel,
            _open: open,
            _message: message,
        }
    }

    pub(super) fn ready(&self) -> bool {
        self.channel.ready_state() == RtcDataChannelState::Open
            && self.channel.buffered_amount() <= WAITING
    }

    pub(super) fn send(&self, frame: &CameraFrame) -> Option<usize> {
        if !self.ready() {
            return None;
        }

        let message = encode(frame);
        let sent =
            protocol::parts(&message).all(|part| self.channel.send_with_u8_array(&part).is_ok());

        sent.then_some(frame.data.len())
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        self.channel.close();
    }
}
