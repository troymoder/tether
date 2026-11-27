use std::collections::HashMap;
use std::time::Duration;

use android::apps::Package;
use leptos::prelude::*;
use protocol::{App, DeviceId, Request, Variant};

use super::called;
use crate::app::Global;
use crate::link::{Link, ServerStoreFields};
use crate::ui::action::use_action_many;
use crate::ui::loadable::Loadable;
use crate::ui::poll::use_poll;

/// Apps start, stop and get installed on a device by other means, so it is
/// asked again this often.
const LOOKS_EVERY: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Standing {
    Absent,
    /// The device has it, and the registry has no build for its architectures.
    Unfit,
    Current,
    /// The device has another build, either an older one of the registry's or
    /// one installed from elsewhere. `Installed::build` tells them apart.
    Behind,
    Installing,
    Failed(String),
}

#[derive(Clone, PartialEq)]
struct Found {
    packages: Vec<Package>,
    /// The SHA-256 of each registered app's file, by its path on the device.
    sums: HashMap<String, String>,
}

impl Found {
    fn package(&self, id: &str) -> Option<&Package> {
        self.packages.iter().find(|package| package.id == id)
    }

    fn sum(&self, id: &str) -> Option<&String> {
        self.sums.get(&self.package(id)?.path)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Installed {
    link: Link,
    device: StoredValue<DeviceId>,
    found: Signal<Loadable<Found>>,
    again: Trigger,
    /// By the app's id. An app whose install went through has none.
    installs: RwSignal<HashMap<String, Install>>,
    /// The string is the line the user is told once it is done.
    take: Callback<(Taken, String)>,
    hand: Callback<String>,
}

#[derive(Clone)]
enum Install {
    Running,
    Failed { build: u32, reason: String },
}

enum Taken {
    Data(String),
    App(String),
}

impl Installed {
    pub(crate) fn of(id: DeviceId) -> Self {
        let link = expect_context::<Global>().link;
        let again = Trigger::new();
        // Installing puts an app's file at a new path, so a sum kept by path
        // stays right. Each path is summed once and kept for the next asks.
        let summed = StoredValue::new(HashMap::<String, String>::new());
        let device = StoredValue::new(id);
        let found = use_poll(LOOKS_EVERY, move || {
            let on = untrack(|| crate::device::of(link, device.get_value()));

            again.track();

            async move {
                let packages = on.packages().await?;
                let registered: Vec<String> = link
                    .server
                    .apps()
                    .with_untracked(|apps| apps.iter().map(|app| app.id.clone()).collect());
                let mut sums = summed.try_get_value().unwrap_or_default();

                for package in packages
                    .iter()
                    .filter(|package| registered.contains(&package.id))
                {
                    if !sums.contains_key(&package.path)
                        && let Ok(sum) = on.sum(package).await
                    {
                        sums.insert(package.path.clone(), sum);
                    }
                }

                summed.try_set_value(sums.clone());

                Ok(Found { packages, sums })
            }
        });

        // A menu's entry can be pressed on a second row while the first
        // row's is under way, and neither is dropped.
        let (_, take) = use_action_many(Some("apps"), move |(taken, done): (Taken, String)| {
            let on = untrack(|| crate::device::of(link, device.get_value()));

            async move {
                let said = match taken {
                    Taken::Data(id) => on.clear(&id).await,
                    Taken::App(id) => on.uninstall(&id).await,
                };

                again.notify();

                said.map(|()| done)
            }
        });
        let (_, hand) = use_action_many(None, move |id: String| {
            let from = device.get_value();
            let on = untrack(|| crate::device::of(link, from.clone()));

            async move {
                let apk = match on.pull(&id).await {
                    Ok(apk) => apk,
                    Err(error) => {
                        // The page did the pull by itself, so tetherd is
                        // told of its failure.
                        link.noted("apps", Some(&from), Err(error.clone()));

                        return Err(error);
                    }
                };
                // tetherd logs the call by itself, so its answer is only
                // shown.
                let request = Request::HandOver {
                    id,
                    apk,
                    from: Some(from),
                };
                let said = link.call_large(request).await;

                again.notify();

                said
            }
        });

        Self {
            link,
            device,
            found,
            again,
            installs: RwSignal::new(HashMap::new()),
            take,
            hand,
        }
    }

    pub(crate) fn packages(self) -> Loadable<Vec<Package>> {
        self.found
            .with(|found| found.as_ref().map(|found| found.packages.clone()))
    }

    pub(crate) fn package(self, id: &str) -> Option<Package> {
        self.found
            .with(|found| found.as_ref().ready()?.package(id).cloned())
    }

    pub(crate) fn refresh(self) {
        self.again.notify();
    }

    fn fitting(self, app: &App) -> Option<Variant> {
        let listed = self.device.with_value(|id| self.link.device(id))?;

        app.variant(&listed.described()?.abis).cloned()
    }

    /// None when the app on the device was installed from elsewhere.
    pub(crate) fn build(self, app: &App) -> Option<u32> {
        self.found.with(|found| {
            let sum = found.as_ref().ready()?.sum(&app.id)?;
            let variant = app.variants.iter().find(|variant| variant.hash == *sum)?;

            Some(variant.build)
        })
    }

    pub(crate) fn standing(self, app: &App) -> Loadable<Standing> {
        let fitting = self.fitting(app);
        let install = self
            .installs
            .with(|installs| installs.get(&app.id).cloned());

        self.found.with(|found| {
            found.as_ref().map(|found| {
                let current = fitting
                    .as_ref()
                    .is_some_and(|variant| found.sum(&app.id) == Some(&variant.hash));

                match (install, &fitting) {
                    (Some(Install::Running), _) => return Standing::Installing,
                    // A failure is of one build, and goes once that build is
                    // on the device or another is the one to install.
                    (Some(Install::Failed { build, reason }), Some(variant))
                        if build == variant.build && !current =>
                    {
                        return Standing::Failed(reason);
                    }
                    _ => {}
                }

                match (found.package(&app.id), &fitting) {
                    (None, _) => Standing::Absent,
                    (Some(_), None) => Standing::Unfit,
                    (Some(_), Some(_)) if current => Standing::Current,
                    (Some(_), Some(_)) => Standing::Behind,
                }
            })
        })
    }

    pub(crate) fn install(self, app: &App) {
        let Some(variant) = untrack(|| self.fitting(app)) else {
            return;
        };

        let Installed {
            link,
            again,
            installs,
            ..
        } = self;
        let id = self.device.get_value();
        let on = untrack(|| crate::device::of(link, id.clone()));
        let app_id = app.id.clone();
        let name = called(app);

        installs.update(|installs| {
            installs.insert(app_id.clone(), Install::Running);
        });

        leptos::task::spawn_local(async move {
            let installed = on.install(&app_id).await;
            // A file can install under another package name than the one it
            // was uploaded as, and then the device still lacks this app.
            let arrived = async {
                let packages = on.packages().await.ok()?;
                let package = packages.into_iter().find(|package| package.id == app_id)?;

                on.sum(&package).await.ok()
            }
            .await
            .is_some_and(|sum| sum == variant.hash);
            let done = match installed {
                Ok(()) if !arrived => Err(format!(
                    "install failed: {app_id}: installed package differs from the uploaded build"
                )),
                Ok(()) => Ok(format!(
                    "Installed {name} on {}",
                    untrack(|| link.name(&id))
                )),
                Err(error) => Err(error),
            };

            // The component that asked may be gone by now. The user is told
            // all the same.
            installs.try_update(|installs| match &done {
                Ok(_) => installs.remove(&app_id),
                Err(reason) => installs.insert(
                    app_id.clone(),
                    Install::Failed {
                        build: variant.build,
                        reason: reason.clone(),
                    },
                ),
            });

            again.notify();
            link.did("apps", Some(&id), done);
        });
    }

    pub(crate) fn clear(self, id: String, name: &str) {
        self.take
            .run((Taken::Data(id), format!("Cleared the data of {name}")));
    }

    pub(crate) fn uninstall(self, id: String, name: &str) {
        // The view may have closed while the user was asked to confirm.
        let Some(device) = self.device.try_get_value() else {
            return;
        };

        let device = untrack(|| self.link.name(&device));

        self.take
            .run((Taken::App(id), format!("Took {name} off {device}")));
    }

    pub(crate) fn hand_over(self, id: String) {
        self.hand.run(id);
    }
}
