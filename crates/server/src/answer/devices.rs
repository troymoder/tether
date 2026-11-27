use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use protocol::{At, DeviceId, Means, Pairing, ToClient, Way, mention};
use tokio::time::{Instant, sleep, timeout_at};

use crate::adb::host::{Nearby, Offer};
use crate::adb::{Adb, means_of, serial_in};
use crate::app::App;
use crate::trust;
use crate::watch::look;

const LETTING_GO: Duration = Duration::from_secs(5);

pub(super) async fn rename(app: &Arc<App>, device: &DeviceId, name: &str) -> Result<String> {
    let name = trust::checked_name(name)?;
    let called = match app.device(device) {
        Ok(listed) => listed.name,
        Err(_) => app
            .known
            .name(device)
            .with_context(|| format!("rename failed: {} not found", mention(device)))?,
    };
    let said = app.known.rename(device, &called, name).await?;

    look(app).await;

    Ok(said)
}

/// A connected device would be known again at once.
pub(super) async fn forget(app: &Arc<App>, device: &DeviceId) -> Result<String> {
    ensure!(
        app.device(device)
            .map_or(true, |listed| listed.connected().is_none()),
        "forget failed: disconnect it first"
    );

    let said = app.known.forget(device).await?;

    look(app).await;

    Ok(said)
}

/// A browser connects through its own USB itself, and only asks here whether it
/// may.
pub(super) async fn connect(
    app: &Arc<App>,
    client: &protocol::Client,
    id: &DeviceId,
    way: Option<Way>,
    code: Option<&str>,
    take: bool,
) -> Result<String> {
    // A browser asks right after it reports a phone, which may not be listed
    // yet.
    if app.device(id).is_err() {
        look(app).await;
    }

    let device = app
        .device(id)
        .map_err(|_| anyhow::anyhow!("connect failed: {} not found", mention(id)))?;
    let way = way.or_else(|| device.connected().cloned());
    let route = match &way {
        Some(way) => device.routes.iter().find(|route| route.way == *way),
        None => device.routes.first(),
    }
    .cloned()
    .context("connect failed: no route")?;

    match &route.way.at {
        At::Instance(instance) => ensure!(
            *instance == app.trust.primary(),
            "connect failed: unknown instance"
        ),
        At::Browser(browser) => ensure!(
            browser.0 == client.print,
            "connect failed: another browser's USB"
        ),
    }

    let elsewhere = connections(app, id)
        .into_iter()
        .any(|connected| connected != route.way);

    if elsewhere {
        ensure!(take, "connect failed: in use");
        ensure!(
            let_go(app, id, Some(&route.way)).await,
            "connect failed: in use"
        );
    }

    let name = mention(id);
    let said = if matches!(route.way.at, At::Browser(_)) {
        format!("Connected to {name}")
    } else if let DeviceId::Iphone(udid) = id {
        ensure!(
            app.iphones.want(udid),
            "connect failed: {name} not plugged in"
        );

        format!("Connecting to {name}")
    } else if let DeviceId::Emulator { port, .. } = id
        && let Some(avd) = app.emulators.avd_on(*port)
        && !app.emulators.runs(&avd)
    {
        // Starting one of tetherd's emulators connects to it as it boots.
        app.emulators
            .start(&avd)
            .await
            .with_context(|| format!("connect failed: {name} did not start"))?;

        format!("Starting {name}")
    } else if route.pairing {
        let code = code.context("connect failed: pairing code needed")?;

        reach(&app.adb, &route.address, Some(code), &name).await?;

        match connect_paired(app, &route.address).await {
            Some(_) => format!("Paired with {name} and connected to it"),
            None => format!("Paired with {name}"),
        }
    } else {
        app.adb.connect(&route.address, &name).await?
    };

    look(app).await;

    Ok(said)
}

pub(super) async fn reach_typed(
    app: &Arc<App>,
    address: &str,
    code: Option<&str>,
) -> Result<String> {
    let said = reach(&app.adb, address, code, address).await?;
    let paired = match code {
        Some(_) => connect_paired(app, address).await,
        None => None,
    };

    look(app).await;

    Ok(match paired {
        Some(_) => format!("Paired with {address} and connected to it"),
        None => said,
    })
}

/// It waits for a phone for as long as it runs, which is until the page lets
/// the code go.
pub(crate) async fn pair_scanned(
    app: &Arc<App>,
    name: &str,
    password: &str,
    step: impl Fn(&Pairing),
) -> Pairing {
    if !sane(name) || !sane(password) {
        return Pairing::Failed("pair failed: invalid name or password".to_owned());
    }

    let address = loop {
        let scanned = app
            .adb
            .nearby()
            .into_iter()
            .find(|phone| phone.offer == Offer::Pairing && phone.name == name);

        if let Some(phone) = scanned {
            break phone.address;
        }

        sleep(Duration::from_millis(500)).await;
    };

    step(&Pairing::Scanned {
        address: address.clone(),
    });

    if let Err(error) = app.adb.pair(&address, password).await {
        let said = format!("{error:#}");

        app.journal.done("pair", &Err(said.clone()), None).await;

        return Pairing::Failed(said);
    }

    step(&Pairing::Connecting);

    let connected = connect_paired(app, &address).await;
    let device = connected
        .as_ref()
        .and_then(|phone| serial_in(&phone.name))
        .map(|serial| DeviceId::Android(serial.to_owned()));

    look(app).await;

    let said = match &device {
        Some(_) => format!("Paired with {address} and connected to it"),
        None => format!("Paired with {address}"),
    };

    app.journal.done("pair", &Ok(said), device.as_ref()).await;

    match device {
        Some(device) => Pairing::Connected { device },
        None => Pairing::Paired,
    }
}

pub(super) async fn disconnect(app: &Arc<App>, device: &DeviceId) -> Result<String> {
    app.foreign(device)?;

    let gone = let_go(app, device, None).await;

    look(app).await;

    ensure!(gone, "disconnect failed: in use");

    Ok(format!("Disconnected from {}", mention(device)))
}

fn connections(app: &App, id: &DeviceId) -> Vec<Way> {
    let here = At::Instance(app.trust.primary());
    let serial = id.serial();
    let mut ways: Vec<Way> = app
        .adb
        .addresses_of(&serial)
        .iter()
        .map(|address| Way {
            means: means_of(address),
            at: here.clone(),
        })
        .collect();

    if matches!(id, DeviceId::Iphone(_)) && app.iphones.holds(&serial) {
        ways.push(Way {
            means: Means::Usb,
            at: here,
        });
    }

    ways.extend(app.holders(&serial).into_iter().map(|(_, client)| Way {
        means: Means::Usb,
        at: At::Browser(client),
    }));

    ways
}

/// False if a browser still holds the device after it was asked to let go.
async fn let_go(app: &App, id: &DeviceId, keep: Option<&Way>) -> bool {
    let here = At::Instance(app.trust.primary());
    let serial = id.serial();
    let kept = |way: Way| keep == Some(&way);

    for address in app.adb.addresses_of(&serial) {
        if !kept(Way {
            means: means_of(&address),
            at: here.clone(),
        }) {
            app.adb.let_go(&address);
        }
    }

    let usb = Way {
        means: Means::Usb,
        at: here,
    };

    if matches!(id, DeviceId::Iphone(_)) && app.iphones.holds(&serial) && !kept(usb) {
        app.iphones.let_go(&serial);
    }

    let asked: Vec<_> = app
        .holders(&serial)
        .into_iter()
        .filter(|(_, client)| {
            !kept(Way {
                means: Means::Usb,
                at: At::Browser(client.clone()),
            })
        })
        .map(|(peer, _)| peer)
        .collect();

    for peer in &asked {
        app.topics
            .tell(*peer, &ToClient::LetGo { device: id.clone() });
    }

    let deadline = Instant::now() + LETTING_GO;

    loop {
        // Made before the check so a report in between is not missed.
        let reported = app.reported.notified();
        let holding = app
            .holders(&serial)
            .iter()
            .any(|(peer, _)| asked.contains(peer));

        if !holding {
            return true;
        }

        if timeout_at(deadline, reported).await.is_err() {
            return false;
        }
    }
}

pub(crate) async fn connect_paired(app: &App, paired: &str) -> Option<Nearby> {
    let (host, _) = paired.rsplit_once(':')?;

    // Some phones offer the connection several seconds after pairing.
    for _ in 0..20 {
        let phone = app.adb.nearby().into_iter().find(|phone| {
            phone.offer == Offer::Connecting
                && phone.address.rsplit_once(':').map(|(at, _)| at) == Some(host)
        });

        if let Some(phone) = phone
            && reach(&app.adb, &phone.address, None, &phone.address)
                .await
                .is_ok()
        {
            return Some(phone);
        }

        sleep(Duration::from_millis(500)).await;
    }

    None
}

async fn reach(adb: &Adb, address: &str, code: Option<&str>, name: &str) -> Result<String> {
    ensure!(
        sane(address) && code.is_none_or(sane),
        "invalid address or code"
    );
    // A typo caught here does not use up one of the phone's attempts. A code
    // typed in is the six digits the phone shows, and a QR code's password
    // (`pair_scanned`) is not typed.
    ensure!(
        code.is_none_or(|code| code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit())),
        "the pairing code is six digits"
    );

    match code {
        // Pairing only makes the phone trust this key. It is connected to
        // afterwards.
        Some(code) => {
            adb.pair(address, code).await?;

            Ok(format!("Paired with {name}"))
        }
        None => adb.connect(address, name).await,
    }
}

/// True if an address or pairing code holds only the characters of a host,
/// a port or a code, so it cannot smuggle anything into the URI libadb parses.
fn sane(text: &str) -> bool {
    text.starts_with(|first: char| first.is_ascii_alphanumeric() || first == '[')
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".:-[]".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_only_addresses_and_codes_that_are_safe_as_arguments() {
        assert!(sane("192.168.1.30:46693"));
        assert!(sane("[fe80::1]:5555"));
        assert!(sane("123456"));
        assert!(!sane(""));
        assert!(!sane("-rf"));
        assert!(!sane("host:1 x"));
        assert!(!sane("a;b"));
    }
}
