use std::sync::Arc;

use anyhow::{Context, Result};
use protocol::EmulatorSettings;

use crate::app::App;
use crate::watch::look;

pub(super) async fn create(
    app: &Arc<App>,
    name: &str,
    settings: EmulatorSettings,
) -> Result<String> {
    app.emulators.create(name, settings).await?;

    look(app).await;

    Ok(format!("Made {name}"))
}

pub(super) async fn configure(app: &App, name: &str, settings: EmulatorSettings) -> Result<String> {
    app.emulators.configure(name, settings).await?;

    Ok(format!("Saved. It applies the next time {name} starts."))
}

pub(super) async fn delete(app: &Arc<App>, name: &str) -> Result<String> {
    app.emulators.delete(name).await?;

    look(app).await;

    Ok(format!("Removed {name}"))
}

pub(super) async fn erase(app: &App, name: &str) -> Result<String> {
    app.emulators.erase(name).await?;

    Ok(format!("Erased {name}"))
}

pub(super) async fn start(app: &Arc<App>, name: &str) -> Result<String> {
    app.emulators
        .start(name)
        .await
        .with_context(|| format!("failed to start {name}"))?;

    look(app).await;

    Ok(format!("Starting {name}"))
}

pub(super) async fn stop(app: &Arc<App>, name: &str) -> Result<String> {
    app.emulators.stop(name, true).await?;

    look(app).await;

    Ok(format!("{name} has stopped"))
}
