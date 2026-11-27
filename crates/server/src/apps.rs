//! One app may be handed over several times, built for different ABIs. Those
//! builds are variants of the one app, and a device gets the variant built for
//! it.

mod variants;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use protocol::{App, Device, DeviceId, ToClient, Topic, Variant, mention};
use sha2::{Digest, Sha256};

use self::variants::{abis, built_for, called, fits, supersede};
use crate::adb::Adb;
use crate::app::abis_of;
use crate::db::Db;
use crate::topics::Topics;

/// The registry lives in the database alone and is read from there each time.
pub(crate) struct Apps {
    folder: PathBuf,
    db: Arc<Db>,
    topics: Arc<Topics>,
    aapt2: Option<PathBuf>,
}

/// A hand-over or an add that did not finish leaves the registry and the folder
/// out of step.
fn sweep(folder: &Path, registry: &mut Vec<App>) -> Result<()> {
    for app in registry.iter_mut() {
        app.variants
            .retain(|variant| folder.join(&variant.file).exists());
    }

    registry.retain(|app| !app.variants.is_empty());

    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        let named = |name: &str| {
            registry
                .iter()
                .flat_map(|app| &app.variants)
                .any(|variant| variant.file == name)
        };

        if let Some(name) = path.file_name().and_then(|name| name.to_str())
            && name.ends_with(".apk")
            && !named(name)
        {
            std::fs::remove_file(&path)?;
        }
    }

    Ok(())
}

impl Apps {
    pub(crate) async fn new(
        folder: PathBuf,
        aapt2: Option<PathBuf>,
        db: Arc<Db>,
        topics: Arc<Topics>,
    ) -> Result<Self> {
        std::fs::create_dir_all(&folder)?;

        let before = db.apps().await?;
        let mut after = before.clone();
        sweep(&folder, &mut after)?;

        for app in &before {
            match after.iter().find(|kept| kept.id == app.id) {
                None => {
                    db.forget_app(&app.id).await?;
                }
                Some(kept) if kept != app => {
                    let variants = kept.variants.clone();

                    db.change_app(&app.id, false, |app| {
                        app.variants = variants;

                        Ok(())
                    })
                    .await?;
                }
                Some(_) => {}
            }
        }

        Ok(Self {
            folder,
            db,
            topics,
            aapt2,
        })
    }

    pub(crate) async fn list(&self) -> Result<Vec<App>> {
        self.db.apps().await
    }

    async fn find(&self, id: &str) -> Result<App> {
        self.db
            .app(id)
            .await?
            .with_context(|| format!("{id}: no build uploaded"))
    }

    /// The next start sweeps the file away if nothing renames or removes it.
    async fn stage(&self, prefix: &str, apk: &[u8]) -> Result<PathBuf> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default();
        let staged = self.folder.join(format!(".{prefix}-{stamp}.apk"));
        tokio::fs::write(&staged, apk).await?;

        Ok(staged)
    }

    pub(crate) async fn hand_over(&self, id: &str, apk: &[u8]) -> Result<String> {
        let abis = abis(apk);
        // Staged under a temporary name and renamed to its build's file before
        // the registry names it, so an install never reads half an APK.
        let handing = self.stage("handing", apk).await?;
        let folder = self.folder.clone();
        // A build stays registered without its version when aapt2 is
        // missing or cannot read it.
        let version = match &self.aapt2 {
            Some(aapt2) => crate::icon::version(aapt2, &handing)
                .await
                .inspect_err(|error| tracing::warn!("{id}: {error:#}"))
                .ok()
                .flatten(),
            None => None,
        };

        let (gone, built_for) = self
            .change(id, true, |app| {
                let build = app.builds.saturating_add(1);
                let file = format!("{id}@{build}.apk");
                std::fs::rename(&handing, folder.join(&file))?;
                app.builds = build;

                let new = Variant {
                    file,
                    abis,
                    build,
                    version,
                    handed_over: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|since| since.as_millis() as u64)
                        .unwrap_or_default(),
                    size: apk.len() as u64,
                    hash: format!("{:x}", Sha256::digest(apk)),
                };
                let gone = supersede(&mut app.variants, &new);
                let built_for = built_for(&new.abis);

                app.variants.push(new);

                Ok((gone, built_for))
            })
            .await
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&handing);
            })?;

        // Removed only once the registry no longer names them.
        for old in gone {
            let _ = tokio::fs::remove_file(self.folder.join(&old.file)).await;
        }

        // A new build may bring a new label and icon. The build stays
        // registered even if they cannot be read.
        if let Err(error) = self.describe(id).await {
            tracing::warn!("{id}: {error:#}");
        }

        Ok(format!(
            "Added {}, for {built_for}",
            called(&self.find(id).await?)
        ))
    }

    pub(crate) async fn package(&self, id: &str, build: u32) -> Result<Vec<u8>> {
        let app = self.find(id).await?;
        let variant = app
            .variants
            .iter()
            .find(|variant| variant.build == build)
            .with_context(|| format!("{id}: no build {build}"))?;

        Ok(tokio::fs::read(self.folder.join(&variant.file)).await?)
    }

    /// Replaces what is installed even when that has a higher version. Older
    /// builds are handed over on purpose, and builds of one version for
    /// different ABIs are numbered apart. Android allows the downgrade for a
    /// debuggable app and on an emulator.
    pub(crate) async fn install(&self, adb: &Adb, device: &Device, id: &str) -> Result<String> {
        ensure!(
            device.ready(),
            "install failed: {} not ready",
            mention(&device.id)
        );

        let runs = abis_of(device)?;
        let app = self.find(id).await?;
        let variant = app.variant(runs).with_context(|| {
            format!(
                "{id}: no build for {} ({})",
                mention(&device.id),
                runs.join(", ")
            )
        })?;
        let apk = tokio::fs::read(self.folder.join(&variant.file)).await?;
        adb.install(&device.id.serial(), &apk).await?;

        Ok(format!(
            "Installed {} on {}",
            called(&app),
            mention(&device.id)
        ))
    }

    pub(crate) async fn add(&self, apk: &[u8], onto: Option<&Device>) -> Result<String> {
        if let Some(device) = onto {
            fits(device, apk)?;
        }

        let aapt2 = self
            .aapt2
            .as_ref()
            .context("no Android SDK: cannot read the app label")?;

        // aapt2 reads from a file.
        let incoming = self.stage("incoming", apk).await?;
        let id = crate::icon::package_name(aapt2, &incoming).await;
        let _ = tokio::fs::remove_file(&incoming).await;
        let id = id?;

        ensure!(crate::name::valid_name(&id), "invalid package name: {id}");

        let said = self.hand_over(&id, apk).await?;
        tracing::info!("{said}");

        Ok(id)
    }

    async fn describe(&self, id: &str) -> Result<()> {
        let Some(aapt2) = &self.aapt2 else {
            return Ok(());
        };

        let app = self.find(id).await?;
        let newest = app.latest().context("app has no builds")?;
        let about = crate::icon::read(aapt2, &self.folder.join(&newest.file)).await?;

        // A registered package name is safe as a file name.
        let file = format!("{id}.svg");
        let icon = match about.icon {
            Ok(svg) => {
                tokio::fs::write(self.folder.join(&file), svg).await?;

                Some(file)
            }
            // An older build's icon would be stale, so it goes too.
            Err(_) => {
                let _ = tokio::fs::remove_file(self.folder.join(&file)).await;

                None
            }
        };

        self.change(id, false, |app| {
            app.name = about.name;
            app.icon = icon;

            Ok(())
        })
        .await
    }

    /// Reads the label and icon of each app registered before they were kept,
    /// or while there was no SDK to read them with.
    pub(crate) async fn describe_all(&self) {
        for app in self.list().await.unwrap_or_default() {
            if app.name.is_none()
                && let Err(error) = self.describe(&app.id).await
            {
                tracing::warn!("{}: {error:#}", app.id);
            }
        }
    }

    pub(crate) async fn icon(&self, id: &str) -> Result<String> {
        let app = self.find(id).await?;
        let icon = app.icon.with_context(|| format!("{id}: no usable icon"))?;

        Ok(tokio::fs::read_to_string(self.folder.join(icon)).await?)
    }

    pub(crate) async fn forget(&self, id: &str) -> Result<()> {
        // The files go once the registry no longer names them. A later
        // hand-over numbers its builds from 1 again, into the same file names.
        let app = self.db.forget_app(id).await?;

        self.topics.send(
            |topic| *topic == Topic::Apps,
            &ToClient::AppGone(id.to_owned()),
        );

        for variant in &app.variants {
            let _ = tokio::fs::remove_file(self.folder.join(&variant.file)).await;
        }

        if let Some(icon) = &app.icon {
            let _ = tokio::fs::remove_file(self.folder.join(icon)).await;
        }

        Ok(())
    }

    pub(crate) async fn set_auto(&self, id: &str, device: &DeviceId, on: bool) -> Result<()> {
        self.change(id, false, |app| {
            app.set_auto(device, on);

            Ok(())
        })
        .await
    }

    async fn change<T>(
        &self,
        id: &str,
        new: bool,
        edit: impl FnOnce(&mut App) -> Result<T>,
    ) -> Result<T> {
        let done = self.db.change_app(id, new, edit).await?;

        match self.db.app(id).await {
            Ok(Some(app)) => self
                .topics
                .send(|topic| *topic == Topic::Apps, &ToClient::App(app)),
            Ok(None) => {}
            Err(error) => tracing::error!("db failed: {error:#}"),
        }

        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::variants::tests::{apk, app, built};
    use super::*;

    #[tokio::test]
    async fn two_hand_overs_at_once_get_builds_of_their_own() {
        let folder = std::env::temp_dir().join(format!("tetherd-hand-{}", std::process::id()));
        let apps = Apps::new(
            folder.clone(),
            None,
            Arc::new(Db::in_memory().await),
            Arc::default(),
        )
        .await
        .unwrap();
        let arm = apk(&["lib/arm64-v8a/a.so"]);
        let x86 = apk(&["lib/x86_64/a.so"]);

        let (first, second) = tokio::join!(
            apps.hand_over("com.example", &arm),
            apps.hand_over("com.example", &x86),
        );
        first.unwrap();
        second.unwrap();

        let app = apps.find("com.example").await.unwrap();
        assert_eq!(app.builds, 2);

        let mut builds: Vec<u32> = app.variants.iter().map(|variant| variant.build).collect();
        builds.sort();
        assert_eq!(builds, [1, 2]);

        for variant in &app.variants {
            assert!(folder.join(&variant.file).exists(), "{}", variant.file);
        }

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn sweeps_the_folder_and_the_registry() {
        let folder = std::env::temp_dir().join(format!("tetherd-sweep-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();

        for file in [
            "com.example@2.apk",
            "com.example@1.apk",
            ".incoming-7.apk",
            "com.example.svg",
        ] {
            std::fs::write(folder.join(file), b"x").unwrap();
        }

        let named = |file: &str| Variant {
            file: file.to_owned(),
            ..built(1, &[])
        };

        let mut registry = vec![
            app(vec![named("com.example@2.apk"), named("com.example@3.apk")]),
            App {
                id: "com.gone".to_owned(),
                ..app(vec![named("com.gone@1.apk")])
            },
        ];

        sweep(&folder, &mut registry).unwrap();

        let left: Vec<_> = registry
            .iter()
            .flat_map(|app| &app.variants)
            .map(|variant| &variant.file)
            .collect();
        assert_eq!(left, ["com.example@2.apk"]);

        let mut files: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        assert_eq!(files, ["com.example.svg", "com.example@2.apk"]);

        std::fs::remove_dir_all(&folder).unwrap();
    }
}
