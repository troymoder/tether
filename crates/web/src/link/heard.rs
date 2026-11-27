use js_sys::{ArrayBuffer, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Event, EventTarget, MessageEvent};

/// The listeners are removed when this is dropped, so nothing hears events
/// from a connection that is gone.
pub(crate) struct Heard {
    target: EventTarget,
    listeners: Vec<(&'static str, Listener)>,
}

type Listener = Closure<dyn Fn(Event)>;

impl Heard {
    pub(crate) fn new(target: &EventTarget) -> Self {
        Self {
            target: target.clone(),
            listeners: Vec::new(),
        }
    }

    pub(crate) fn on(&mut self, kind: &'static str, then: impl Fn(Event) + 'static) {
        let listener = Listener::new(then);
        let _ = self
            .target
            .add_event_listener_with_callback(kind, listener.as_ref().unchecked_ref());

        self.listeners.push((kind, listener));
    }
}

impl Drop for Heard {
    fn drop(&mut self) {
        for (kind, listener) in &self.listeners {
            let _ = self
                .target
                .remove_event_listener_with_callback(kind, listener.as_ref().unchecked_ref());
        }
    }
}

/// True for a text message, which tetherd sends to end a channel because
/// everything else it sends is binary (`Hub::close` in
/// crates/server/src/rtc.rs).
pub(crate) fn ends(event: &Event) -> bool {
    event
        .dyn_ref::<MessageEvent>()
        .is_some_and(|message| message.data().is_string())
}

pub(crate) fn bytes(event: &Event) -> Option<Vec<u8>> {
    let buffer = event
        .dyn_ref::<MessageEvent>()?
        .data()
        .dyn_into::<ArrayBuffer>()
        .ok()?;

    Some(Uint8Array::new(&buffer).to_vec())
}
