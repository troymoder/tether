use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use protocol::{EmulatorOptions, EmulatorSettings};
use tokio::process::Command;

use super::Emulators;
use crate::adb::output;

/// The way an emulator reaches the host GPU. It needs an X server for that,
/// even without a window.
pub(super) enum Gpu {
    /// macOS, where it needs none.
    Direct,
    /// A Wayland session, by its socket's name in `XDG_RUNTIME_DIR`. The
    /// emulator gets an X server of its own that draws through it.
    Wayland {
        satellite: PathBuf,
        socket: OsString,
    },
    /// An X session, whose server tetherd's own `DISPLAY` names.
    X,
}

impl Emulators {
    fn home(&self) -> Option<PathBuf> {
        Some(self.sdk.as_ref()?.join("libexec/android-sdk"))
    }

    pub(super) fn avds(&self) -> PathBuf {
        self.data.join("android/avd")
    }

    pub(super) fn tool(&self, path: &str) -> Result<Command> {
        let sdk = self.sdk.as_ref().context("no Android SDK")?;
        let mut command = Command::new(sdk.join(path));

        command
            .env("ANDROID_HOME", sdk.join("libexec/android-sdk"))
            .env("ANDROID_USER_HOME", self.data.join("android"))
            .env("ANDROID_AVD_HOME", self.avds());

        Ok(command)
    }

    /// None on a machine with no desktop, which renders in software. Looked up
    /// at each start, because tetherd may have started before the desktop did.
    pub(super) fn gpu(&self) -> Option<Gpu> {
        if cfg!(target_os = "macos") {
            return Some(Gpu::Direct);
        }

        if let Some(socket) = wayland_socket() {
            let satellite = self.satellite.clone()?;

            return Some(Gpu::Wayland { satellite, socket });
        }

        set("DISPLAY").map(|_| Gpu::X)
    }

    async fn profiles(&self) -> Vec<String> {
        let Ok(mut avdmanager) = self.tool("bin/avdmanager") else {
            return Vec::new();
        };

        output(avdmanager.args(["list", "device", "-c"]))
            .await
            .unwrap_or_default()
            .lines()
            .filter(|id| id.starts_with("pixel"))
            .map(str::to_owned)
            .collect()
    }

    fn images(&self) -> Vec<String> {
        let Some(home) = self.home() else {
            return Vec::new();
        };

        let folders = |path: PathBuf| {
            let mut names: Vec<String> = std::fs::read_dir(path)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect();

            names.sort();

            names
        };

        let root = home.join("system-images");
        let mut images = Vec::new();

        for api in folders(root.clone()) {
            for kind in folders(root.join(&api)) {
                for abi in folders(root.join(&api).join(&kind)) {
                    images.push(format!("system-images;{api};{kind};{abi}"));
                }
            }
        }

        images
    }

    /// Read once, because avdmanager starts a JVM.
    pub(crate) async fn options(&self) -> Option<&EmulatorOptions> {
        self.options
            .get_or_init(|| async {
                self.sdk.as_ref()?;

                Some(EmulatorOptions {
                    profiles: self.profiles().await,
                    images: self.images(),
                    feeds_cameras: self.shim.is_some(),
                })
            })
            .await
            .as_ref()
    }

    pub(super) async fn check(&self, settings: &EmulatorSettings) -> Result<()> {
        let android = self.options().await.context("no Android SDK")?;

        ensure!(
            android.profiles.contains(&settings.profile),
            "unknown device profile: {}",
            settings.profile
        );

        ensure!(
            android.images.contains(&settings.image),
            "system image not installed: {}",
            settings.image
        );

        ensure!(
            (2048..=16384).contains(&settings.memory),
            "memory must be 2-16 GB"
        );

        ensure!(
            settings.camera.is_none() || android.feeds_cameras,
            "camera shim not found: libvcam.so"
        );

        for camera in settings.camera.iter().map(|camera| camera.settings) {
            ensure!(
                protocol::CAMERA_SIZES.contains(&(camera.width, camera.height)),
                "unsupported camera resolution: {}x{}",
                camera.width,
                camera.height
            );

            ensure!(
                protocol::CAMERA_RATES.contains(&camera.fps),
                "unsupported camera rate: {}",
                camera.fps
            );
        }

        Ok(())
    }
}

fn set(variable: &str) -> Option<OsString> {
    std::env::var_os(variable).filter(|value| !value.is_empty())
}

/// tetherd's own `WAYLAND_DISPLAY` if it has one. A service started before the
/// desktop never had the variable, so the runtime folder is searched for a
/// socket a compositor answers on.
fn wayland_socket() -> Option<OsString> {
    let runtime = PathBuf::from(set("XDG_RUNTIME_DIR")?);

    if let Some(named) = set("WAYLAND_DISPLAY") {
        return Some(named);
    }

    let names = std::fs::read_dir(&runtime)
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.file_name()));

    wayland_sockets(names)
        .into_iter()
        .find(|name| answers(&runtime.join(name)))
}

/// The names a compositor gives its sockets, lowest first. Each has a
/// `.lock` file beside it, which is left out.
fn wayland_sockets(names: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let mut sockets: Vec<OsString> = names
        .filter(|name| {
            name.to_str()
                .and_then(|name| name.strip_prefix("wayland-"))
                .is_some_and(|number| {
                    !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
                })
        })
        .collect();

    sockets.sort();

    sockets
}

/// A compositor that crashed leaves its socket behind.
fn answers(socket: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(socket).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_compositors_sockets_are_taken_lowest_first() {
        let names = [
            "wayland-1.lock",
            "wayland-1",
            "bus",
            "wayland-0",
            "wayland-",
            "wayland-x",
        ];
        let found = wayland_sockets(names.into_iter().map(OsString::from));

        assert_eq!(found, ["wayland-0", "wayland-1"].map(OsString::from));
    }
}
