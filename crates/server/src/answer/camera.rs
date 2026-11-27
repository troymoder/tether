use anyhow::Result;

use crate::app::App;

pub(super) fn take(app: &App, emulator: &str, client: &protocol::Client) -> Result<String> {
    let by = app
        .emulators
        .take_camera(emulator, (&client.print, &client.name))?;

    app.remotes.stop(emulator);
    app.streams
        .taken(emulator, &protocol::CameraHeld { by, taken: true });

    Ok(format!("Feeding the camera of {emulator}"))
}

pub(super) fn stop_remote(app: &App, emulator: &str) -> String {
    if app.remotes.stop(emulator) {
        app.emulators.black_camera(emulator);
    }

    format!("Stopped feeding the camera of {emulator} from tetherd")
}
