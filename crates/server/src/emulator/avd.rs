use std::process::Stdio;
use std::sync::Mutex;

use anyhow::{Context, Result, ensure};
use protocol::EmulatorSettings;
use tokio::io::AsyncWriteExt;

use super::log::keep;
use super::{Emulators, Entry, Saved};
use crate::name::valid_name;

impl Emulators {
    pub(crate) async fn create(&self, name: &str, settings: EmulatorSettings) -> Result<()> {
        let _busy = self.busy.lock().await;

        ensure!(
            valid_name(name),
            "invalid name: use letters, digits, '.', '-', '_'"
        );

        self.check(&settings).await?;

        {
            let mut list = self.list.lock().unwrap();

            ensure!(
                list.iter().all(|entry| entry.saved.name != name),
                "emulator already exists: {name}"
            );

            // The emulator takes even console ports from 5554 to 5682.
            let port = (5554..=5682)
                .step_by(2)
                .find(|port| list.iter().all(|entry| entry.saved.port != *port))
                .context("no free emulator port (5554-5682)")?;

            list.push(Entry::stopped(Saved {
                name: name.to_owned(),
                port,
                settings,
            }));
        }

        self.save().await?;
        self.changed(name);

        Ok(())
    }

    pub(crate) async fn configure(&self, name: &str, settings: EmulatorSettings) -> Result<()> {
        let (_, running) = self.find(name)?;
        // A running emulator was started with the old settings.
        ensure!(
            running.is_none(),
            "{name} is running: stop it to change settings"
        );

        self.check(&settings).await?;

        {
            let mut list = self.list.lock().unwrap();

            if let Some(entry) = list.iter_mut().find(|entry| entry.saved.name == name) {
                entry.saved.settings = settings;
            }
        }

        self.save().await?;
        self.changed(name);

        Ok(())
    }

    pub(crate) async fn erase(&self, name: &str) -> Result<()> {
        let _busy = self.busy.lock().await;
        let (_, running) = self.find(name)?;

        ensure!(running.is_none(), "{name} is running: stop it first");
        self.remove_avd(name).await;

        Ok(())
    }

    pub(crate) async fn delete(&self, name: &str) -> Result<()> {
        let _busy = self.busy.lock().await;
        let (_, running) = self.find(name)?;

        ensure!(running.is_none(), "{name} is running: stop it first");
        self.remove_avd(name).await;

        self.list
            .lock()
            .unwrap()
            .retain(|entry| entry.saved.name != name);

        self.save().await?;
        self.changed(name);

        Ok(())
    }

    async fn remove_avd(&self, name: &str) {
        let avds = self.avds();

        let _ = tokio::fs::remove_dir_all(avds.join(format!("{name}.avd"))).await;
        let _ = tokio::fs::remove_file(avds.join(format!("{name}.ini"))).await;
        let _ = tokio::fs::remove_file(avds.join(format!("{name}.made"))).await;
    }

    /// A virtual device is made for one image and one phone, so it is made
    /// again when either changes, which wipes what was installed on it.
    pub(super) async fn make_avd(
        &self,
        name: &str,
        settings: &EmulatorSettings,
        log: &Mutex<String>,
    ) -> Result<()> {
        let avds = self.avds();
        let made = format!("{} {}", settings.image, settings.profile);
        let marker = avds.join(format!("{name}.made"));

        if tokio::fs::read_to_string(&marker).await.ok().as_ref() == Some(&made) {
            return Ok(());
        }

        keep(
            log,
            &format!("Making a {} from {}", settings.profile, settings.image),
        );

        self.remove_avd(name).await;
        tokio::fs::create_dir_all(&avds).await?;

        let mut avdmanager = self
            .tool("bin/avdmanager")?
            .args(["create", "avd", "--name", name])
            .args(["--package", &settings.image, "--device", &settings.profile])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // It asks whether to make a custom hardware profile.
        if let Some(mut stdin) = avdmanager.stdin.take() {
            stdin.write_all(b"no\n").await?;
        }

        let made_it = avdmanager.wait_with_output().await?;

        ensure!(
            made_it.status.success(),
            "{}",
            String::from_utf8_lossy(&made_it.stderr).trim()
        );

        tokio::fs::write(&marker, &made).await?;

        Ok(())
    }

    /// Android lists its cameras once as it boots, and the quick-boot snapshot
    /// keeps that list, so a camera that came, went or changed is only seen
    /// after a cold boot. The cameras a snapshot was made with are kept beside
    /// it, and a snapshot without a match is removed before this boot. Keeping
    /// it for one boot would load it under the new list if that boot dies
    /// before saving.
    pub(super) async fn drop_stale_snapshot(&self, name: &str, cameras: &str) -> Result<()> {
        let folder = self.avds().join(format!("{name}.avd"));
        let snapshot = folder.join("snapshots/default_boot");
        let before = tokio::fs::read_to_string(folder.join("cameras")).await.ok();

        if snapshot.exists() && before.as_deref() != Some(cameras) {
            tokio::fs::remove_dir_all(&snapshot).await?;
        }

        tokio::fs::write(folder.join("cameras"), cameras).await?;

        Ok(())
    }
}
