use std::sync::Arc;
use std::time::Duration;

use protocol::DeviceId;
use tokio::sync::mpsc;

use super::Out;
use crate::app::App;

pub(super) async fn watch(
    app: &Arc<App>,
    out: &Out,
    device: &DeviceId,
    mut from: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let console = match device {
        DeviceId::Emulator { port, .. } => Some(*port),
        _ => None,
    };

    // A booting emulator has no video yet, and a stopping one has none any
    // more.
    let pictured = || {
        let connected = app.adb.connected(&device.serial());
        let (port, pid, stopping) = app.emulators.pictured(console?, connected)?;

        (stopping || !app.is_ready(device)).then_some((port, pid))
    };

    let (pictures, mut taken) = mpsc::channel::<Vec<u8>>(1);
    // An emulator that is being started has no process while its virtual
    // device and camera are made, so it is polled until it has one.
    let showing = async {
        loop {
            if let Some((port, pid)) = pictured() {
                crate::boot::show(port, pid, &pictures, || pictured().is_some()).await;
            }

            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    };

    let passing = async {
        while let Some(picture) = taken.recv().await {
            out.put(&picture);
        }
    };

    let closed = async { while from.recv().await.is_some() {} };

    tokio::select! {
        () = showing => {}
        () = passing => {}
        () = closed => {}
    }
}
