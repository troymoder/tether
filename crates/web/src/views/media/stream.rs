//! A photo or video is streamed through the service worker in
//! glue/serviceWorker/index.ts, so the browser asks for the byte ranges it
//! needs instead of the page reading the whole file first.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise};

use crate::device::Device;

/// glue/serviceWorker/index.ts asks for chunks of the same size.
pub(super) const CHUNK: u64 = 1024 * 1024;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = tetherStream)]
    fn ready() -> js_sys::Promise;

    #[wasm_bindgen(js_namespace = tetherStream)]
    fn open(size: f64, kind: &str, read: &Closure<dyn Fn(f64, f64) -> js_sys::Promise>) -> String;

    #[wasm_bindgen(js_namespace = tetherStream)]
    fn close(address: &str);
}

pub(super) struct Stream {
    pub(super) address: String,
    // Kept alive while glue/registerServiceWorker.ts may still call it.
    _read: Closure<dyn Fn(f64, f64) -> js_sys::Promise>,
}

impl Drop for Stream {
    fn drop(&mut self) {
        close(&self.address);
    }
}

pub(super) async fn start(
    device: &dyn Device,
    path: String,
    size: u64,
    kind: &str,
    served: RwSignal<u64>,
) -> Result<Option<Stream>, String> {
    let ready = JsFuture::from(ready())
        .await
        .ok()
        .and_then(|ready| ready.as_bool());

    if ready != Some(true) {
        return Ok(None);
    }

    // The stream gets its own helper, so its reads never wait behind the
    // gallery's thumbnails.
    let helper = device
        .gallery()
        .await
        .map_err(|error| format!("stream failed: {error}"))?;
    let read =
        Closure::<dyn Fn(f64, f64) -> js_sys::Promise>::new(move |offset: f64, length: f64| {
            let helper = helper.clone();
            let path = path.clone();

            future_to_promise(async move {
                let bytes = helper
                    .read(&path, offset as u64, length as u64)
                    .await
                    .map_err(|error| JsValue::from_str(&error))?;

                served.try_update(|served| *served += bytes.len() as u64);

                Ok(js_sys::Uint8Array::from(bytes.as_slice()).into())
            })
        });

    Ok(Some(Stream {
        address: open(size as f64, kind, &read),
        _read: read,
    }))
}

pub(super) async fn whole(
    device: &dyn Device,
    path: &str,
    size: u64,
    fetched: RwSignal<u64>,
) -> Result<Vec<u8>, String> {
    let helper = device.gallery().await?;
    let mut file = Vec::new();

    while (file.len() as u64) < size {
        let bytes = helper.read(path, file.len() as u64, CHUNK).await?;

        if bytes.is_empty() {
            break;
        }

        file.extend_from_slice(&bytes);
        fetched.try_set(file.len() as u64);
    }

    Ok(file)
}

pub(super) fn kind(name: &str) -> &'static str {
    let extension = name.rsplit('.').next().unwrap_or_default().to_lowercase();

    match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "heic" => "image/heic",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "3gp" => "video/3gpp",
        _ => "application/octet-stream",
    }
}
