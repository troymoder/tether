use anyhow::Result;
use protocol::{DeviceId, Place};

use crate::app::App;
use crate::location;

pub(super) async fn geo_fix(
    app: &App,
    device: &DeviceId,
    latitude: f64,
    longitude: f64,
) -> Result<String> {
    let device = app.reached(device)?;

    location::geo_fix(&app.adb, &device, latitude, longitude).await
}

pub(super) async fn keep(app: &App, place: Place) -> Result<String> {
    let name = place.name.clone();

    app.locations.keep(place).await?;

    Ok(format!("Kept {name}"))
}

pub(super) async fn forget(app: &App, name: &str) -> Result<String> {
    app.locations.forget(name).await?;

    Ok(format!("Forgot {name}"))
}
