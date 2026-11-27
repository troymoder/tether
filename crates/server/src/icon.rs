mod badging;
mod resources;

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use android::icon::{Element, Resource, image_kind};
use anyhow::{Context, Result};
use tokio::process::Command;

use crate::adb::output;

pub(crate) fn tool(sdk: &Path) -> Option<PathBuf> {
    let mut tools: Vec<PathBuf> = std::fs::read_dir(sdk.join("libexec/android-sdk/build-tools"))
        .ok()?
        .filter_map(|version| Some(version.ok()?.path().join("aapt2")))
        .filter(|tool| tool.exists())
        .collect();
    tools.sort();

    tools.pop()
}

struct Package<'a> {
    aapt2: &'a Path,
    path: &'a Path,
    bytes: Vec<u8>,
    /// Empty for an icon that is an image, which needs none.
    resources: String,
}

impl Package<'_> {
    async fn dump(&self, what: &[&str]) -> Result<String> {
        let mut command = Command::new(self.aapt2);
        command.arg("dump").args(what).arg(self.path);

        output(&mut command).await
    }
}

impl android::icon::Package for Package<'_> {
    fn resource(&self, reference: &str) -> Option<Resource> {
        resources::resource(&self.resources, reference)
    }

    async fn xml(&self, path: &str) -> Result<Element> {
        let tree = self.dump(&["xmltree", "--file", path]).await?;

        Element::parse(&tree).with_context(|| format!("{path} is empty"))
    }

    async fn file(&self, path: &str) -> Result<Vec<u8>> {
        let mut archive = zip::ZipArchive::new(Cursor::new(&self.bytes))?;
        let mut bytes = Vec::new();

        archive.by_name(path)?.read_to_end(&mut bytes)?;

        Ok(bytes)
    }
}

pub(crate) struct About {
    pub name: Option<String>,
    pub icon: Result<String>,
}

pub(crate) async fn package_name(aapt2: &Path, apk: &Path) -> Result<String> {
    let badging = output(Command::new(aapt2).args(["dump", "badging"]).arg(apk)).await?;

    Ok(badging::named(&badging)
        .context("APK has no package name")?
        .to_owned())
}

pub(crate) async fn version(aapt2: &Path, apk: &Path) -> Result<Option<String>> {
    let badging = output(Command::new(aapt2).args(["dump", "badging"]).arg(apk)).await?;

    Ok(badging::version(&badging).map(str::to_owned))
}

pub(crate) async fn read(aapt2: &Path, apk: &Path) -> Result<About> {
    let mut package = Package {
        aapt2,
        path: apk,
        bytes: tokio::fs::read(apk).await?,
        resources: String::new(),
    };
    let badging = package.dump(&["badging"]).await?;
    let name = badging::label(&badging);

    let Some(file) = badging::listed(&badging) else {
        return Ok(About {
            name,
            icon: Err(anyhow::anyhow!("app has no icon")),
        });
    };

    if image_kind(file).is_none() {
        package.resources = match package.dump(&["resources"]).await {
            Ok(resources) => resources,
            Err(error) => {
                return Ok(About {
                    name,
                    icon: Err(error),
                });
            }
        };
    }

    Ok(About {
        icon: android::icon::svg(&package, file).await,
        name,
    })
}
