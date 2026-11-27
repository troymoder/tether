mod apps;
mod camera;
mod devices;
mod emulators;
mod places;

pub(crate) use devices::pair_scanned;

use std::sync::Arc;

use anyhow::Result;
use protocol::{ArchivedRequest, DeviceId, EmulatorSettings, Place, Way, own};

use crate::app::App;

/// The request is read in place so an app or file in it is not copied.
pub(crate) async fn answer(
    app: &Arc<App>,
    request: &ArchivedRequest,
    client: &protocol::Client,
) -> Result<String> {
    match request {
        ArchivedRequest::Approve { print } => app.trust.approve(print).await,
        ArchivedRequest::Reject { print } => app.trust.reject(print),
        ArchivedRequest::Revoke { print } => {
            let said = app.trust.revoke(print).await?;

            for peer in app.topics.peers(print) {
                app.hub.leave(peer);
            }

            Ok(said)
        }
        ArchivedRequest::Rename { print, name } => app.trust.rename(print, name).await,
        ArchivedRequest::RenameInstance { name } => app.trust.rename_instance(name).await,
        ArchivedRequest::RenameDevice { device, name } => {
            devices::rename(app, &own(device), name).await
        }
        ArchivedRequest::ForgetDevice { device } => devices::forget(app, &own(device)).await,
        ArchivedRequest::Connect {
            device,
            way,
            code,
            take,
        } => {
            let way = way.as_ref().map(own::<Way>);
            let code = code.as_ref().map(|code| code.as_str());

            devices::connect(app, client, &own(device), way, code, *take).await
        }
        ArchivedRequest::Reach { address, code } => {
            devices::reach_typed(app, address, code.as_ref().map(|code| code.as_str())).await
        }
        ArchivedRequest::Disconnect { device } => devices::disconnect(app, &own(device)).await,
        ArchivedRequest::CreateEmulator { name, settings } => {
            emulators::create(app, name, own::<EmulatorSettings>(settings)).await
        }
        ArchivedRequest::Configure { name, settings } => {
            emulators::configure(app, name, own::<EmulatorSettings>(settings)).await
        }
        ArchivedRequest::Delete { name } => emulators::delete(app, name).await,
        ArchivedRequest::Erase { name } => emulators::erase(app, name).await,
        ArchivedRequest::Start { name } => emulators::start(app, name).await,
        ArchivedRequest::Stop { name } => emulators::stop(app, name).await,
        ArchivedRequest::EmulatorLog { name } => app.emulators.log(name),
        ArchivedRequest::HandOver { id, apk, .. } => apps::hand_over(app, id, apk).await,
        ArchivedRequest::AddApp { apk, device } => {
            apps::add(app, apk, device.as_ref().map(own).as_ref()).await
        }
        ArchivedRequest::InstallApp { device, id } => {
            app.apps
                .install(&app.adb, &app.reached(&own(device))?, id)
                .await
        }
        ArchivedRequest::AutoInstall { id, device, on } => {
            apps::auto_install(app, id, &own(device), *on).await
        }
        ArchivedRequest::ForgetApp { id } => apps::forget(app, id).await,
        ArchivedRequest::AppIcon { id } => app.apps.icon(id).await,
        ArchivedRequest::GeoFix {
            device,
            latitude,
            longitude,
        } => {
            places::geo_fix(
                app,
                &own(device),
                latitude.to_native(),
                longitude.to_native(),
            )
            .await
        }
        ArchivedRequest::KeepPlace(place) => places::keep(app, own::<Place>(place)).await,
        ArchivedRequest::ForgetPlace { name } => places::forget(app, name).await,
        ArchivedRequest::TakeCamera { emulator } => camera::take(app, emulator, client),
        ArchivedRequest::StopRemote { emulator } => Ok(camera::stop_remote(app, emulator)),
    }
}

pub(crate) async fn noted(
    app: &Arc<App>,
    request: &ArchivedRequest,
    client: &protocol::Client,
) -> std::result::Result<String, String> {
    // Looked up before, as a deleted emulator is gone after, and again after,
    // as a created one is only there then.
    let before = acted_on(app, request);
    let result = answer(app, request, client)
        .await
        .map_err(|error| format!("{error:#}"));
    let device = before.or_else(|| acted_on(app, request));

    app.journal
        .answered(request, &result, device.as_ref())
        .await;

    result
}

fn acted_on(app: &App, request: &ArchivedRequest) -> Option<DeviceId> {
    match request {
        ArchivedRequest::Connect { device, .. }
        | ArchivedRequest::Disconnect { device }
        | ArchivedRequest::RenameDevice { device, .. }
        | ArchivedRequest::ForgetDevice { device }
        | ArchivedRequest::InstallApp { device, .. }
        | ArchivedRequest::AutoInstall { device, .. }
        | ArchivedRequest::GeoFix { device, .. } => Some(own(device)),
        ArchivedRequest::HandOver { from: device, .. } | ArchivedRequest::AddApp { device, .. } => {
            device.as_ref().map(own)
        }
        ArchivedRequest::CreateEmulator { name, .. }
        | ArchivedRequest::Configure { name, .. }
        | ArchivedRequest::Delete { name }
        | ArchivedRequest::Erase { name }
        | ArchivedRequest::Start { name }
        | ArchivedRequest::Stop { name }
        | ArchivedRequest::EmulatorLog { name }
        | ArchivedRequest::TakeCamera { emulator: name }
        | ArchivedRequest::StopRemote { emulator: name } => Some(DeviceId::Emulator {
            port: app.emulators.port_of(name)?,
            instance: app.trust.primary(),
        }),
        ArchivedRequest::Approve { .. }
        | ArchivedRequest::Reject { .. }
        | ArchivedRequest::Revoke { .. }
        | ArchivedRequest::Rename { .. }
        | ArchivedRequest::RenameInstance { .. }
        | ArchivedRequest::Reach { .. }
        | ArchivedRequest::ForgetApp { .. }
        | ArchivedRequest::KeepPlace(_)
        | ArchivedRequest::ForgetPlace { .. }
        | ArchivedRequest::AppIcon { .. } => None,
    }
}
