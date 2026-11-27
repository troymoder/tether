use js_sys::{Array, Object, Reflect, Uint8Array};
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{CryptoKeyPair, EcKeyGenParams, EcdsaParams, SubtleCrypto};

use crate::app::storage::{store_key, stored_key};

fn subtle() -> Option<SubtleCrypto> {
    Some(window().crypto().ok()?.subtle())
}

async fn done(promise: Result<js_sys::Promise, JsValue>) -> Option<JsValue> {
    JsFuture::from(promise.ok()?).await.ok()
}

fn usages() -> Array {
    Array::of2(&"sign".into(), &"verify".into())
}

/// This browser's adb key as PKCS#8, made the first time. adb signs a raw
/// 20-byte token, which WebCrypto's RSA signing cannot do because it always
/// hashes first. So the key is extractable and the page signs with it
/// itself (usb.rs).
pub(super) async fn adb_key() -> Option<Vec<u8>> {
    if let Some(kept) = stored_key("adb").await {
        return Some(Uint8Array::new(&kept).to_vec());
    }

    let algorithm = Object::new();
    let fields: [(&str, JsValue); 4] = [
        ("name", "RSASSA-PKCS1-v1_5".into()),
        ("modulusLength", 2048.into()),
        (
            "publicExponent",
            Uint8Array::from([1, 0, 1].as_slice()).into(),
        ),
        ("hash", "SHA-1".into()),
    ];

    for (name, value) in fields {
        Reflect::set(&algorithm, &name.into(), &value).ok()?;
    }

    let subtle = subtle()?;
    let pair: CryptoKeyPair = done(subtle.generate_key_with_object(&algorithm, true, &usages()))
        .await?
        .unchecked_into();
    let pkcs8 = done(subtle.export_key("pkcs8", &pair.get_private_key())).await?;
    let kept = store_key("adb", pkcs8).await?;

    Some(Uint8Array::new(&kept).to_vec())
}

/// The private half is not extractable, so not even this page can export
/// it. IndexedDB stores the key object itself.
async fn pair() -> Option<CryptoKeyPair> {
    if let Some(kept) = stored_key("pair").await {
        return Some(kept.unchecked_into());
    }

    let algorithm = EcKeyGenParams::new("ECDSA", "P-256");
    let made = done(subtle()?.generate_key_with_object(&algorithm, false, &usages())).await?;

    Some(store_key("pair", made).await?.unchecked_into())
}

/// This browser's public key as SPKI and its signature of `bytes` (r and s,
/// 32 bytes each).
pub(super) async fn signed(bytes: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let pair = pair().await?;
    let subtle = subtle()?;
    let key = done(subtle.export_key("spki", &pair.get_public_key())).await?;
    let algorithm = EcdsaParams::new_with_str("ECDSA", "SHA-256");
    let signature =
        done(subtle.sign_with_object_and_u8_array(&algorithm, &pair.get_private_key(), bytes))
            .await?;

    Some((
        Uint8Array::new(&key).to_vec(),
        Uint8Array::new(&signature).to_vec(),
    ))
}

pub(super) fn browser_name() -> String {
    let agent = window().navigator().user_agent().unwrap_or_default();
    let browser = [
        ("Edg/", "Edge"),
        ("Firefox/", "Firefox"),
        ("Chrome/", "Chrome"),
        ("Safari/", "Safari"),
    ]
    .into_iter()
    .find(|(mark, _)| agent.contains(mark))
    .map_or("Browser", |(_, name)| name);
    let system = [
        ("Android", "Android"),
        ("iPhone", "iOS"),
        ("iPad", "iOS"),
        ("Mac OS", "macOS"),
        ("Windows", "Windows"),
        ("CrOS", "ChromeOS"),
        ("Linux", "Linux"),
    ]
    .into_iter()
    .find(|(mark, _)| agent.contains(mark))
    .map_or("", |(_, name)| name);

    if system.is_empty() {
        browser.to_owned()
    } else {
        format!("{browser} on {system}")
    }
}
