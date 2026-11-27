//! The codec uses the protocol crate's own encoding. Each message crosses as
//! the plain object serde makes of it, typed by tsify.

use rkyv::Archive;
use rkyv::api::high::HighValidator;
use rkyv::bytecheck::CheckBytes;
use rkyv::de::Pool;
use rkyv::rancor::{Error, Strategy};
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

use protocol::{
    Admitted, Connect, Hello, Media, Pairing, Picture, Screen, ToClient, ToServer, access, encode,
    own, squeeze,
};

fn decode<T>(bytes: &[u8]) -> Result<Ts<T>, JsError>
where
    T: Archive + Tsify + serde::Serialize,
    T::Archived:
        for<'a> CheckBytes<HighValidator<'a, Error>> + rkyv::Deserialize<T, Strategy<Pool, Error>>,
{
    let archived = access::<T>(bytes).map_err(|error| JsError::new(&error))?;

    Ok(own::<T>(archived).into_ts()?)
}

fn result<T: serde::de::DeserializeOwned>(result: JsValue) -> Result<Result<T, String>, JsError> {
    Ok(tsify::serde_wasm_bindgen::from_value(result)?)
}

/// A whole message to the page. Cut it with `parts` to send it.
#[wasm_bindgen(js_name = encodeToClient)]
pub fn encode_to_client(message: Ts<ToClient>) -> Result<Vec<u8>, JsError> {
    Ok(encode(&message.to_rust()?))
}

/// A message from the page on the control channel, once gathered.
#[wasm_bindgen(js_name = decodeToServer)]
pub fn decode_to_server(bytes: &[u8]) -> Result<Ts<ToServer>, JsError> {
    decode::<ToServer>(bytes)
}

/// The body that answers `POST /rtc/hello`.
#[wasm_bindgen(js_name = encodeHello)]
pub fn encode_hello(hello: Ts<Hello>) -> Result<Vec<u8>, JsError> {
    Ok(encode(&hello.to_rust()?))
}

/// The body the page posts to `/rtc/connect`.
#[wasm_bindgen(js_name = decodeConnect)]
pub fn decode_connect(bytes: &[u8]) -> Result<Ts<Connect>, JsError> {
    decode::<Connect>(bytes)
}

/// The body that answers `POST /rtc/connect`.
#[wasm_bindgen(js_name = encodeAdmitted)]
pub fn encode_admitted(admitted: Ts<Admitted>) -> Result<Vec<u8>, JsError> {
    Ok(encode(&admitted.to_rust()?))
}

/// The job of a media channel: its first message, once gathered.
#[wasm_bindgen(js_name = decodeMedia)]
pub fn decode_media(bytes: &[u8]) -> Result<Ts<Media>, JsError> {
    decode::<Media>(bytes)
}

/// A call's outcome with a line for the user, as a `Call` lane is answered.
#[wasm_bindgen(js_name = encodeResult)]
pub fn encode_result(
    #[wasm_bindgen(unchecked_param_type = "{ Ok: string } | { Err: string }")] outcome: JsValue,
) -> Result<Vec<u8>, JsError> {
    Ok(encode(&result::<String>(outcome)?))
}

/// A picture on a `Watch` lane, packed from RGBA pixels as tetherd does.
#[wasm_bindgen(js_name = encodePicture)]
pub fn encode_picture(width: u16, height: u16, rgba: &[u8]) -> Vec<u8> {
    encode(&Picture {
        width,
        height,
        data: squeeze(rgba),
    })
}

/// A file tetherd sends whole on a lane, such as the Android helper's dex.
#[wasm_bindgen(js_name = encodeFile)]
pub fn encode_file(file: Vec<u8>) -> Vec<u8> {
    encode(&Ok::<Vec<u8>, String>(file))
}

/// A message on a `Screen` lane: the codec, the clipboard, or why it failed.
#[wasm_bindgen(js_name = encodeScreen)]
pub fn encode_screen(screen: Ts<Screen>) -> Result<Vec<u8>, JsError> {
    Ok(encode(&screen.to_rust()?))
}

/// A step of pairing on a `Pair` lane.
#[wasm_bindgen(js_name = encodePairing)]
pub fn encode_pairing(pairing: Ts<Pairing>) -> Result<Vec<u8>, JsError> {
    Ok(encode(&pairing.to_rust()?))
}

/// The first message on an adb channel: whether the service opened.
#[wasm_bindgen(js_name = encodeOpened)]
pub fn encode_opened(
    #[wasm_bindgen(unchecked_param_type = "{ Ok: null } | { Err: string }")] outcome: JsValue,
) -> Result<Vec<u8>, JsError> {
    Ok(encode(&result::<()>(outcome)?))
}

/// A message cut into the parts the control channel and the lanes carry.
#[wasm_bindgen]
pub fn parts(message: &[u8]) -> Vec<js_sys::Uint8Array> {
    protocol::parts(message)
        .map(|part| js_sys::Uint8Array::from(part.as_slice()))
        .collect()
}

/// Puts the parts of one channel's messages together again.
#[wasm_bindgen]
#[derive(Default)]
pub struct Gather(protocol::Gather);

#[wasm_bindgen]
impl Gather {
    /// A gather with nothing in it yet.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Gather {
        Gather::default()
    }

    /// The whole message once its last part has come, undefined before then.
    pub fn add(&mut self, part: &[u8]) -> Option<Vec<u8>> {
        self.0.add(part)
    }
}
