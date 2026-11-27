//! tetherd's settings come from the `--config` files it is given, each over
//! the one before, and then from the flags.

use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

const DEFAULT_STUN: &str = "stun.l.google.com:19302";

/// The flags tetherd takes. Each one can also be set in a config file.
#[derive(clap::Parser, Serialize)]
#[command(version, about = "Serves tether's page and the devices it shows")]
pub(crate) struct Flags {
    /// The port it serves on.
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    /// A TOML file of settings, over the ones before. May be given again.
    #[arg(long, value_name = "FILE")]
    #[serde(skip)]
    config: Vec<PathBuf>,
    /// Asks for a setup code even when clients are already trusted (trust.rs).
    #[arg(long)]
    #[serde(skip)]
    pub(crate) setup: bool,
    /// The folder for tetherd's database and stored files.
    #[arg(long, value_name = "DIR")]
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<PathBuf>,
    /// The folder with the built page.
    #[arg(long, value_name = "DIR")]
    #[serde(skip_serializing_if = "Option::is_none")]
    web: Option<PathBuf>,
    /// scrcpy's server jar, of the version nix/android.nix pins.
    #[arg(long, value_name = "JAR")]
    #[serde(skip_serializing_if = "Option::is_none")]
    scrcpy_server: Option<PathBuf>,
    /// The helper (crates/android/helper), as a .dex file.
    #[arg(long, value_name = "DEX")]
    #[serde(skip_serializing_if = "Option::is_none")]
    android_helper: Option<PathBuf>,
    /// An SDK with the emulator and system images in it.
    #[arg(long, value_name = "DIR")]
    #[serde(skip_serializing_if = "Option::is_none")]
    android_sdk: Option<PathBuf>,
    /// xwayland-satellite, which gives emulators that use the GPU an X server
    /// on Linux.
    #[arg(long, value_name = "FILE")]
    #[serde(skip_serializing_if = "Option::is_none")]
    xwayland_satellite: Option<PathBuf>,
    /// The camera shim, `libvcam.so` (crates/vcam).
    #[arg(long, value_name = "FILE")]
    #[serde(skip_serializing_if = "Option::is_none")]
    vcam: Option<PathBuf>,
    /// ffmpeg, which decodes the video a camera is fed and captures this
    /// machine's webcams. Without it, the one on the PATH.
    #[arg(long, value_name = "FILE")]
    #[serde(skip_serializing_if = "Option::is_none")]
    ffmpeg: Option<PathBuf>,
    /// The STUN server that tells this machine its address as seen from
    /// outside, so a browser behind another NAT can reach it, as `host:port`.
    /// Empty for none. Without it, Google's (`DEFAULT_STUN`).
    #[arg(long, value_name = "HOST:PORT")]
    #[serde(skip_serializing_if = "Option::is_none")]
    stun: Option<String>,
    /// Another `host:port` the page is served from, such as the dev server in
    /// the dev stack. tetherd then redirects requests for the page there. May
    /// be given again.
    #[arg(long = "origin", value_name = "HOST:PORT")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    origins: Vec<String>,
    /// The log filter: a level, or directives such as `info,tetherd=debug`.
    #[arg(long, value_name = "FILTER")]
    #[serde(skip_serializing_if = "Option::is_none")]
    log: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub(crate) port: u16,
    pub(crate) data: Option<PathBuf>,
    /// Needed unless the page comes from one of `origins`.
    pub(crate) web: Option<PathBuf>,
    pub(crate) scrcpy_server: PathBuf,
    pub(crate) android_helper: PathBuf,
    pub(crate) android_sdk: Option<PathBuf>,
    pub(crate) xwayland_satellite: Option<PathBuf>,
    pub(crate) vcam: Option<PathBuf>,
    pub(crate) ffmpeg: Option<PathBuf>,
    pub(crate) stun: Option<String>,
    #[serde(default)]
    pub(crate) origins: Vec<String>,
    pub(crate) log: Option<String>,
}

impl Settings {
    pub(crate) fn read(flags: &Flags) -> Result<Self> {
        let mut sources = config::Config::builder();

        for file in &flags.config {
            sources = sources.add_source(config::File::from(file.as_path()));
        }

        sources
            .add_source(config::Config::try_from(flags)?)
            .build()?
            .try_deserialize()
            .context("invalid settings")
    }
}

pub(crate) fn stun(said: Option<String>) -> Option<String> {
    match said {
        None => Some(DEFAULT_STUN.to_owned()),
        Some(stun) => Some(stun).filter(|stun| !stun.is_empty()),
    }
}

/// Without one given, the shim is looked for beside this program, where cargo
/// builds it.
pub(crate) fn shim(said: Option<PathBuf>) -> Option<PathBuf> {
    let beside = || {
        std::env::current_exe()
            .ok()?
            .with_file_name("libvcam.so")
            .canonicalize()
            .ok()
    };

    said.or_else(beside).filter(|shim| shim.is_file())
}

/// The folder holds the trusted clients, so it is refused when another user
/// owns it.
pub(crate) fn data_folder(said: Option<PathBuf>) -> Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    let data = match said {
        Some(data) => data,
        None => std::env::var_os("XDG_DATA_HOME")
            .filter(|home| !home.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .context("data is not set, and neither is HOME")?
            .join("tetherd"),
    };

    // The emulator needs an absolute path.
    let data = std::path::absolute(data)?;

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&data)
        .with_context(|| data.display().to_string())?;

    let owner = std::fs::metadata(&data)?.uid();
    ensure!(
        owner == rustix::process::getuid().as_raw(),
        "{}: owned by another user",
        data.display()
    );

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_flags_over_files() {
        let folder = std::env::temp_dir().join(format!("tetherd-settings-{}", std::process::id()));

        std::fs::create_dir_all(&folder).unwrap();

        let defaults = folder.join("defaults.toml");
        let config = folder.join("config.toml");

        std::fs::write(
            &defaults,
            "port = 1\nscrcpy_server = \"/jar\"\nandroid_helper = \"/dex\"\nvcam = \"/vcam\"",
        )
        .unwrap();
        std::fs::write(&config, "port = 2\norigins = [\"localhost:5175\"]").unwrap();

        let flags = <Flags as clap::Parser>::parse_from([
            "tetherd",
            "3",
            "--config",
            defaults.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
        ]);
        let settings = Settings::read(&flags).unwrap();

        std::fs::remove_dir_all(&folder).unwrap();

        assert_eq!(settings.port, 3);
        assert_eq!(settings.scrcpy_server, std::path::Path::new("/jar"));
        assert_eq!(
            settings.vcam.as_deref(),
            Some(std::path::Path::new("/vcam"))
        );
        assert_eq!(settings.origins, ["localhost:5175"]);

        // A misspelled key is refused, so it cannot be silently ignored.
        let folder = std::env::temp_dir().join(format!("tetherd-typo-{}", std::process::id()));

        std::fs::create_dir_all(&folder).unwrap();

        let typo = folder.join("typo.toml");

        std::fs::write(
            &typo,
            "scrcpy_server = \"/jar\"\nandroid_helper = \"/dex\"\nvcma = \"/vcam\"",
        )
        .unwrap();

        let flags = <Flags as clap::Parser>::parse_from([
            "tetherd",
            "3",
            "--config",
            typo.to_str().unwrap(),
        ]);
        let read = Settings::read(&flags);

        std::fs::remove_dir_all(&folder).unwrap();

        assert!(read.is_err());
    }

    #[test]
    fn chooses_the_stun_server() {
        assert_eq!(stun(None).as_deref(), Some(DEFAULT_STUN));
        assert_eq!(stun(Some(String::new())), None);
        assert_eq!(
            stun(Some("stun.example.com:3478".to_owned())).as_deref(),
            Some("stun.example.com:3478")
        );
    }
}
