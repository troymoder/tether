use std::sync::Arc;

use protocol::{Pairing, encode};
use tokio::sync::mpsc;

use super::Out;
use crate::app::App;

pub(super) async fn serve(
    app: &Arc<App>,
    out: &Out,
    (name, password): (&str, &str),
    mut from: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let tell = |step: &Pairing| out.put(&encode(step));
    let pairing = crate::answer::pair_scanned(app, name, password, tell);
    // The page sends nothing on it, so its end is the channel closing.
    let closed = async { while from.recv().await.is_some() {} };

    tokio::select! {
        last = pairing => out.answer(&encode(&last)),
        () = closed => {}
    }
}
