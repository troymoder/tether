use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use protocol::{Feeder, Lens};
use tokio::time::{sleep, timeout};

use super::Emulators;
use crate::camera::Fed;

impl Emulators {
    /// `print` is None when tetherd feeds the camera from its own machine.
    pub(crate) fn feeding(&self, name: &str, lens: Lens, print: Option<&str>) -> Option<Arc<Fed>> {
        let list = self.list.lock().unwrap();
        let entry = list.iter().find(|entry| entry.saved.name == name)?;
        let running = entry.running.as_ref()?;

        if entry.feeder.as_ref().map(|feeder| feeder.print.as_str()) != print {
            return None;
        }

        running.cameras[lens.at()].clone()
    }

    /// Fails with the other client if one already feeds the camera. With
    /// `for_tetherd` no client feeds it afterwards, so tetherd can feed it from
    /// its own machine as this client asked.
    pub(crate) fn claim_camera(
        &self,
        name: &str,
        (print, client): (&str, &str),
        for_tetherd: bool,
    ) -> Result<(), Feeder> {
        let mut list = self.list.lock().unwrap();
        let Some(entry) = list.iter_mut().find(|entry| entry.saved.name == name) else {
            return Ok(());
        };

        if let Some(other) = entry.feeder.as_ref().filter(|other| other.print != print) {
            return Err(other.clone());
        }

        if for_tetherd {
            entry.feeder = None;
        } else if entry.feeder.is_none() {
            entry.feeder = Some(feeder(print, client));
        }

        Ok(())
    }

    pub(crate) fn take_camera(&self, name: &str, (print, client): (&str, &str)) -> Result<Feeder> {
        let mut list = self.list.lock().unwrap();
        let entry = list
            .iter_mut()
            .find(|entry| entry.saved.name == name)
            .with_context(|| format!("emulator not found: {name}"))?;

        ensure!(entry.running.is_some(), "{name} is not running");

        let by = feeder(print, client);

        entry.feeder = Some(by.clone());

        Ok(by)
    }

    pub(crate) fn black_camera(&self, name: &str) {
        let list = self.list.lock().unwrap();

        for entry in list
            .iter()
            .filter(|entry| entry.saved.name == name && entry.feeder.is_none())
        {
            for camera in entry
                .running
                .iter()
                .flat_map(|running| running.cameras.iter())
                .flatten()
            {
                camera.black();
            }
        }
    }

    /// A `name` of None means every camera the client fed, as when it left.
    /// With `black` the cameras go black at once instead of after the silence
    /// vcam.rs waits for.
    pub(crate) fn leave_camera(&self, name: Option<&str>, print: &str, black: bool) {
        let mut list = self.list.lock().unwrap();

        for entry in list.iter_mut() {
            let fed = entry
                .feeder
                .as_ref()
                .is_some_and(|feeder| feeder.print == print);

            if fed && name.is_none_or(|name| entry.saved.name == name) {
                entry.feeder = None;

                let cameras = entry
                    .running
                    .iter()
                    .flat_map(|running| running.cameras.iter());

                for camera in cameras.flatten().filter(|_| black) {
                    camera.black();
                }
            }
        }
    }
}

fn feeder(print: &str, name: &str) -> Feeder {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);

    Feeder {
        print: print.to_owned(),
        name: name.to_owned(),
        since,
    }
}

/// The emulator shows the camera's picture to the device rotated by its turn.
pub(super) async fn follow_turns(console: u16, camera: Weak<Fed>) {
    let mut upright = true;

    loop {
        sleep(Duration::from_millis(700)).await;

        let Some(camera) = camera.upgrade() else {
            return;
        };

        // The timeout stops a hung console from keeping the camera alive.
        let said = timeout(
            Duration::from_secs(5),
            crate::adb::host::console(console, "sensor get acceleration"),
        )
        .await;

        if let Some(now) = said.ok().and_then(Result::ok).as_deref().and_then(stands)
            && now != upright
        {
            camera.turn(now);
            upright = now;
        }
    }
}

/// Upright pulls along the long side, as in `acceleration = 0:9.81:0`, and
/// lying on a side pulls along the short one.
fn stands(said: &str) -> Option<bool> {
    let mut pull = said
        .split_once('=')?
        .1
        .trim()
        .split(':')
        .map(|part| part.trim().parse::<f32>().ok());
    let across = pull.next()??;
    let along = pull.next()??;

    Some(along.abs() >= across.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_which_way_an_emulator_is_turned() {
        assert_eq!(stands("acceleration = 0:9.81:0\nOK"), Some(true));
        assert_eq!(stands("acceleration = -9.81:-1.90735e-06:0"), Some(false));
        assert_eq!(stands("acceleration = 0:-9.81:0"), Some(true));
        assert_eq!(stands("KO: unknown command"), None);
    }
}
