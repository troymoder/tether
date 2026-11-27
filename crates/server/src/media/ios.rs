//! Operations on an iPhone tetherd holds are performed here with its pair
//! record, so the key never leaves tetherd.

use std::sync::Arc;

use ios::perform::{Done, perform};
use protocol::{IosOperation, encode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::Out;
use crate::app::App;

const CHUNK: usize = 64 * 1024;

pub(super) async fn serve(
    app: &Arc<App>,
    out: &Out,
    device: &str,
    operation: &IosOperation,
    from: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let done = match app.iphones.phone(device).await {
        Ok(phone) => perform(&phone, operation).await,
        Err(error) => Err(error),
    };

    match done {
        Err(error) => out.answer(&encode(&Err::<Vec<u8>, String>(format!("{error:#}")))),
        Ok(Done::Nothing) => out.answer(&encode(&Ok::<Vec<u8>, String>(Vec::new()))),
        Ok(Done::Value(value)) => out.answer(&encode(&Ok::<Vec<u8>, String>(value))),
        Ok(Done::Stream(stream)) => {
            out.put(&encode(&Ok::<Vec<u8>, String>(Vec::new())));
            carry(out, stream, from).await;
            out.close();
        }
    }
}

async fn carry(
    out: &Out,
    stream: Box<dyn ios::idevice::ReadWrite>,
    mut from: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let (mut reading, mut writing) = tokio::io::split(stream);
    let to_page = async {
        let mut buffer = vec![0; CHUNK];

        while let Ok(read) = reading.read(&mut buffer).await {
            if read == 0 {
                break;
            }

            out.put(&buffer[..read]);
        }
    };
    let to_phone = async {
        while let Some(message) = from.recv().await {
            if writing.write_all(&message).await.is_err() {
                break;
            }
        }
    };

    tokio::select! {
        () = to_page => {}
        () = to_phone => {}
    }
}
