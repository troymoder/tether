use js_sys::Uint8Array;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, HtmlAnchorElement, Url};

/// Release the address with `Url::revoke_object_url`.
pub(crate) fn address(data: &[u8]) -> Option<String> {
    let parts = js_sys::Array::of1(&Uint8Array::from(data));
    let blob = Blob::new_with_u8_array_sequence(&parts).ok()?;

    Url::create_object_url_with_blob(&blob).ok()
}

pub(crate) fn save(name: &str, data: &[u8]) {
    let Some(address) = address(data) else {
        return;
    };

    let Ok(anchor) = document().create_element("a") else {
        return;
    };

    let anchor: HtmlAnchorElement = anchor.unchecked_into();

    anchor.set_href(&address);
    anchor.set_download(name);
    anchor.click();
    let _ = Url::revoke_object_url(&address);
}

// shortcut: a file is in memory twice while it is sent, here and in the
// message. That is fine for an app or a video. Split it up if larger files are sent.
pub(crate) async fn read(file: &Blob) -> Option<Vec<u8>> {
    let buffer = JsFuture::from(file.array_buffer()).await.ok()?;

    Some(Uint8Array::new(&buffer).to_vec())
}
