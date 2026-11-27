//! tetherd cannot reach the phones on this browser's WebUSB, so the page
//! fetches the build from tetherd's registry and installs it itself, when
//! asked and as each new build arrives for a phone with auto-install on.

use std::collections::HashMap;

use leptos::prelude::*;
use leptos::task::spawn_local;
use protocol::{DeviceId, Media, access, own};

use super::super::{Link, ServerStoreFields};

impl Link {
    /// The error has no prefix, because installing and downloading both fetch
    /// the package.
    pub(crate) async fn package(self, app: &str, build: u32) -> Result<Vec<u8>, String> {
        let asked = Media::Build {
            app: app.to_owned(),
            build,
        };
        let Some(lane) = self.lane(&asked) else {
            return Err("not connected to the server".to_owned());
        };
        let Some(answer) = lane.next().await else {
            return Err("connection to the server lost".to_owned());
        };

        access::<Result<Vec<u8>, String>>(&answer)
            .map(own::<Result<Vec<u8>, String>>)
            .map_err(|error| error.to_string())?
    }

    pub(crate) async fn install_here(self, device: &DeviceId, id: &str) -> Result<String, String> {
        let found = untrack(|| {
            let listed = self.device(device);
            let app = self.server.apps().at_key(id.to_owned()).try_get();

            listed.zip(app)
        });
        let Some((listed, app)) = found else {
            return Err(format!("install failed: {id} or {device} not listed"));
        };

        let Some(described) = listed.described() else {
            return Err(format!("install failed: {} not ready", listed.name));
        };

        let Some(variant) = app.variant(&described.abis) else {
            return Err(format!(
                "install failed: {id}: no build for {} ({})",
                listed.name,
                described.abis.join(", ")
            ));
        };

        let apk = self
            .package(id, variant.build)
            .await
            .map_err(|error| format!("install failed: {error}"))?;
        // A random file name on the phone, so two installs at once do not
        // collide.
        let name = format!("tether-{}", (js_sys::Math::random() * 1e9) as u64);

        android::apps::install(&self.adb(device).await, &apk, &name)
            .await
            .map_err(|error| format!("install failed: {error:#}"))?;

        let called = app.name.as_ref().unwrap_or(&app.id);

        Ok(format!("Installed {called} on {}", listed.name))
    }

    /// The app is reopened where it was in front, because installing stops
    /// it. A new build shows as the app's build count going up.
    pub(super) fn auto_install(self) {
        Effect::new(move |before: Option<HashMap<String, u32>>| {
            let now: HashMap<String, u32> = self.server.apps().with(|apps| {
                apps.iter()
                    .map(|app| (app.id.clone(), app.builds))
                    .collect()
            });

            if let Some(before) = before {
                for (id, builds) in &now {
                    if before.get(id).is_some_and(|had| had < builds) {
                        self.install_new(id);
                    }
                }
            }

            now
        });
    }

    fn install_new(self, id: &str) {
        let phones: Vec<DeviceId> = untrack(|| {
            let Some(app) = self.server.apps().at_key(id.to_owned()).try_get() else {
                return Vec::new();
            };

            self.server.devices().with(|devices| {
                devices
                    .iter()
                    .filter(|device| {
                        device.ready() && device.connected().is_some_and(|way| self.here(way))
                    })
                    .filter(|device| app.auto_on(&device.id))
                    .map(|device| device.id.clone())
                    .collect()
            })
        });

        for phone in phones {
            let id = id.to_owned();

            spawn_local(async move {
                let device = self.adb(&phone).await;
                let shown = android::apps::in_front(&device, &id).await;

                match self.install_here(&phone, &id).await {
                    Ok(said) => self.say(said),
                    Err(said) => {
                        self.say(said);
                        return;
                    }
                }

                if shown && let Err(error) = android::apps::open(&device, &id).await {
                    self.say(format!("open failed: {error:#}"));
                }
            });
        }
    }
}
