use std::sync::Arc;

use anyhow::{Result, ensure};
use protocol::{DeviceId, mention};

use crate::app::App;
use crate::name::valid_name;

pub(super) async fn hand_over(app: &Arc<App>, id: &str, apk: &[u8]) -> Result<String> {
    ensure!(valid_name(id), "invalid package name");

    let said = app.apps.hand_over(id, apk).await?;

    install_by_itself(app, id).await;

    Ok(said)
}

pub(super) async fn add(app: &Arc<App>, apk: &[u8], device: Option<&DeviceId>) -> Result<String> {
    let device = match device {
        Some(device) => Some(app.device(device)?),
        None => None,
    };
    let id = app.apps.add(apk, device.as_ref()).await?;

    install_by_itself(app, &id).await;

    Ok(id)
}

pub(super) async fn auto_install(
    app: &App,
    id: &str,
    device: &DeviceId,
    on: bool,
) -> Result<String> {
    app.apps.set_auto(id, device, on).await?;

    Ok(if on {
        format!("{id} installs itself on {}", mention(device))
    } else {
        format!("{id} no longer installs itself on {}", mention(device))
    })
}

pub(super) async fn forget(app: &App, id: &str) -> Result<String> {
    app.apps.forget(id).await?;

    Ok(format!("Forgot {id}"))
}

/// A device that had the app on screen gets it opened again, because installing
/// stops it.
async fn install_by_itself(app: &Arc<App>, id: &str) {
    let Some(on) = app
        .apps
        .list()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|one| one.id == id)
    else {
        return;
    };

    // A phone on a browser's USB is installed on by that browser.
    for device in app
        .devices()
        .into_iter()
        .filter(|device| app.reached(&device.id).is_ok() && on.auto_on(&device.id))
    {
        let app = app.clone();
        let id = id.to_owned();
        let serial = device.id.serial();

        tokio::spawn(async move {
            let shown = match app.adb.device(&serial) {
                Ok(adb) => android::apps::in_front(&adb, &id).await,
                Err(_) => false,
            };

            match app.apps.install(&app.adb, &device, &id).await {
                Ok(said) => tracing::info!("{said}"),
                Err(error) => {
                    tracing::warn!("install failed: {}: {error:#}", device.name);
                    return;
                }
            }

            if shown && let Ok(adb) = app.adb.device(&serial) {
                let _ = android::apps::open(&adb, &id).await;
            }
        });
    }
}
