use adb::Device;
use adb::libadb::Session;
use anyhow::{Context, Result, bail, ensure};

const STAND_IN: &str = "appops set com.android.shell android:mock_location allow; \
    cmd location providers add-test-provider gps; \
    cmd location providers set-test-provider-enabled gps true";

/// Android before 12 has no test provider commands, so tetherd sets an
/// emulator of one through its console instead.
pub async fn put<S>(device: &Device<S>, latitude: f64, longitude: f64) -> Result<()>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    ensure!(
        (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude),
        "invalid coordinates: latitude -90..90, longitude -180..180"
    );

    let place = format!(
        "cmd location providers set-test-provider-location gps --location {latitude:.6},{longitude:.6}"
    );

    // The test provider is added when setting the location fails, which is
    // the first time and after each reboot.
    for first in [true, false] {
        // The command prints nothing when it succeeds.
        if device
            .script(&place)
            .await
            .is_ok_and(|said| said.trim().is_empty())
        {
            return Ok(());
        }

        if first {
            let _ = device.script(STAND_IN).await;
        }
    }

    bail!("mock location unsupported: requires Android 12+")
}

pub async fn reset<S>(device: &Device<S>) -> Result<()>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    // This fails when there is no test provider, which is the wanted state anyway.
    let _ = device
        .sh([
            "cmd",
            "location",
            "providers",
            "remove-test-provider",
            "gps",
        ])
        .await;

    Ok(())
}

pub async fn whereabouts<S>(device: &Device<S>) -> Result<(f64, f64)>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let dump = device.sh(["dumpsys", "location"]).await?;

    believed(&dump).context("device has no location fix")
}

fn believed(dump: &str) -> Option<(f64, f64)> {
    let mut known = dump
        .split("last location=Location[")
        .skip(1)
        .filter_map(|said| {
            // The provider, then `latitude,longitude`.
            let mut words = said.split([' ', ']']);
            let provider = words.next()?;
            let (latitude, longitude) = words.next()?.split_once(',')?;

            Some((provider, latitude.parse().ok()?, longitude.parse().ok()?))
        })
        .collect::<Vec<(&str, f64, f64)>>();

    known.sort_by_key(|(provider, ..)| *provider != "gps");

    known
        .first()
        .map(|(_, latitude, longitude)| (*latitude, *longitude))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_where_a_device_thinks_it_is() {
        let dump = "  network provider:\n    last location=Location[network 1.500000,2.500000 hAcc=20]\n  \
            gps provider:\n    last location=Location[gps -33.924900,18.424100 hAcc=5.0 et=+1h2m]\n    \
            last coarse location=Location[gps -33.93,18.43 hAcc=2000]\n";

        assert_eq!(believed(dump), Some((-33.9249, 18.4241)));
        assert_eq!(believed("  gps provider:\n    last location=null\n"), None);
        assert_eq!(
            believed("last location=Location[fused 1.5,2.5 hAcc=1]"),
            Some((1.5, 2.5))
        );
    }
}
