//! The address `#<device>/<page>/<position>` keeps the route across a reload
//! or a shared link.

use leptos::prelude::*;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use protocol::DeviceId;

use super::{Ui, Wanted};
use crate::device::{self, Views};
use crate::link::{Link, ServerStoreFields};

const START: &str = "/sdcard";

/// `/` is encoded, since it parts the address.
const PART: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b':')
    .remove(b'@');

const PATH: &AsciiSet = &PART.remove(b'/');

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Route {
    pub device: Option<DeviceId>,
    pub page: Page,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Page {
    /// `emulator` is a virtual device's name.
    Devices {
        emulator: Option<String>,
    },
    Registry {
        app: Option<String>,
    },
    Files {
        folder: String,
    },
    Database {
        app: Option<String>,
        store: Option<String>,
        table: Option<String>,
    },
    Apps,
    Logs,
    Crashes,
    Shell,
    Preferences,
    Media,
    Clipboard,
    Camera,
    Location,
    Settings,
    Access,
    Network,
    Events,
}

impl Page {
    pub(crate) fn devices() -> Self {
        Page::Devices { emulator: None }
    }

    pub(crate) fn registry() -> Self {
        Page::Registry { app: None }
    }

    pub(crate) fn files() -> Self {
        Page::Files {
            folder: START.to_owned(),
        }
    }

    pub(crate) fn database() -> Self {
        Page::Database {
            app: None,
            store: None,
            table: None,
        }
    }

    pub(crate) fn is(&self, other: &Page) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// None for a view that needs no device.
    pub(crate) fn offered(&self) -> Option<Views> {
        Some(match self {
            Page::Apps => Views::APPS,
            Page::Preferences => Views::PREFERENCES,
            Page::Location => Views::LOCATION,
            Page::Files { .. } => Views::FILES,
            Page::Media => Views::MEDIA,
            Page::Camera => Views::CAMERA,
            Page::Database { .. } => Views::DATABASE,
            Page::Clipboard => Views::CLIPBOARD,
            Page::Crashes => Views::CRASHES,
            Page::Logs => Views::LOGS,
            Page::Shell => Views::SHELL,
            Page::Devices { .. }
            | Page::Registry { .. }
            | Page::Events
            | Page::Access
            | Page::Settings
            | Page::Network => return None,
        })
    }

    fn key(&self) -> &'static str {
        match self {
            Page::Devices { .. } => "devices",
            Page::Registry { .. } => "registry",
            Page::Apps => "apps",
            Page::Preferences => "preferences",
            Page::Location => "location",
            Page::Files { .. } => "files",
            Page::Media => "media",
            Page::Camera => "camera",
            Page::Database { .. } => "database",
            Page::Clipboard => "clipboard",
            Page::Crashes => "crashes",
            Page::Logs => "logs",
            Page::Events => "events",
            Page::Shell => "shell",
            Page::Access => "access",
            Page::Settings => "settings",
            Page::Network => "network",
        }
    }

    /// The parts of the address after the page's name, each already encoded.
    fn position(&self) -> Option<String> {
        match self {
            Page::Devices { emulator: at } | Page::Registry { app: at } => at.as_deref().map(part),
            // The folder's own slashes part it in the address too, without
            // the one it starts with.
            Page::Files { folder } => {
                Some(utf8_percent_encode(folder.trim_start_matches('/'), PATH).to_string())
            }
            Page::Database { app, store, table } => {
                let parts: Vec<String> = [app, store, table]
                    .into_iter()
                    .map_while(|at| at.as_deref().map(part))
                    .collect();

                (!parts.is_empty()).then(|| parts.join("/"))
            }
            _ => None,
        }
    }

    fn named(key: &str, position: Option<&str>) -> Option<Self> {
        let at = position.filter(|at| !at.is_empty()).map(whole);

        Some(match key {
            "devices" => Page::Devices { emulator: at },
            "registry" => Page::Registry { app: at },
            "files" => match position {
                Some(folder) => Page::Files {
                    folder: format!("/{}", whole(folder)),
                },
                None => Page::files(),
            },
            "database" => {
                let mut parts = position
                    .unwrap_or_default()
                    .splitn(3, '/')
                    .filter(|at| !at.is_empty())
                    .map(whole);

                Page::Database {
                    app: parts.next(),
                    store: parts.next(),
                    table: parts.next(),
                }
            }
            "apps" => Page::Apps,
            "preferences" => Page::Preferences,
            "location" => Page::Location,
            "media" => Page::Media,
            "camera" => Page::Camera,
            "clipboard" => Page::Clipboard,
            "crashes" => Page::Crashes,
            "logs" => Page::Logs,
            "events" => Page::Events,
            "shell" => Page::Shell,
            "access" => Page::Access,
            "settings" => Page::Settings,
            "network" => Page::Network,
            _ => return None,
        })
    }

    /// A page's position belongs to the device it was on, except in the
    /// registry, which is not about one.
    fn restarted(&self) -> Self {
        match self {
            Page::Devices { .. } => Page::devices(),
            Page::Files { .. } => Page::files(),
            Page::Database { .. } => Page::database(),
            other => other.clone(),
        }
    }
}

fn part(text: &str) -> String {
    utf8_percent_encode(text, PART).to_string()
}

fn whole(part: &str) -> String {
    percent_decode_str(part).decode_utf8_lossy().into_owned()
}

impl Route {
    /// Read before tetherd's state arrives, so that a reload stays on the
    /// same device and page.
    pub(super) fn restored() -> RwSignal<Self> {
        let hash = window().location().hash().unwrap_or_default();

        RwSignal::new(Route::from_address(hash.trim_start_matches('#')))
    }

    fn address(&self) -> String {
        let device = self
            .device
            .as_ref()
            .map(|id| part(&id.to_string()))
            .unwrap_or_default();
        let key = self.page.key();

        match self.page.position() {
            Some(position) => format!("{device}/{key}/{position}"),
            None => format!("{device}/{key}"),
        }
    }

    fn from_address(address: &str) -> Self {
        let (device, rest) = address.split_once('/').unwrap_or((address, ""));
        let (key, position) = match rest.split_once('/') {
            Some((key, position)) => (key, Some(position)),
            None => (rest, None),
        };

        Route {
            device: whole(device).parse().ok(),
            page: Page::named(key, position).unwrap_or_else(Page::devices),
        }
    }
}

/// Components read the route as it resolves against the devices tetherd
/// lists. `stored` is the one the user or the address set.
#[derive(Clone, Copy)]
pub(crate) struct Router {
    stored: RwSignal<Route>,
    device: Memo<Option<DeviceId>>,
    page: Memo<Page>,
    offered: Memo<Option<Views>>,
    loaded: RwSignal<bool>,
}

impl Router {
    pub(super) fn start(stored: RwSignal<Route>, ui: Ui, link: Link) -> Self {
        let offered = Memo::new(move |_| {
            let id = link.about()?;

            Some(device::of(link, id).views().get())
        });
        // Nothing is corrected until tetherd has sent its state, since no
        // device is listed before.
        let resolved = Memo::new(move |_| {
            let route = stored.get();

            if !link.loaded.get() {
                return route;
            }

            let device = link.about();
            // An emulator's page shows only while that emulator is picked
            // and listed. A view the picked device does not offer stays
            // open, and says so in place of the view (app/scope.rs `Needs`).
            let elsewhere = match &route.page {
                Page::Devices {
                    emulator: Some(avd),
                } => link
                    .emulator(avd)
                    .is_none_or(|emulator| Some(emulator.id) != device),
                _ => false,
            };
            let page = if elsewhere {
                Page::devices()
            } else {
                route.page
            };

            Route { device, page }
        });
        let router = Router {
            stored,
            device: Memo::new(move |_| resolved.with(|route| route.device.clone())),
            page: Memo::new(move |_| resolved.with(|route| route.page.clone())),
            offered,
            loaded: link.loaded,
        };

        // Replaces the history entry so switching views adds no back steps. It
        // waits for tetherd's state, since a device that is not listed yet
        // would otherwise be dropped from the address.
        Effect::new(move |_| {
            let address = format!("#{}", resolved.with(Route::address));

            if link.loaded.get()
                && let Ok(history) = window().history()
            {
                let _ = history.replace_state_with_url(
                    &wasm_bindgen::JsValue::NULL,
                    "",
                    Some(&address),
                );
            }
        });

        Effect::new(move |_| {
            let Some(wanted) = ui.wanted.get() else {
                return;
            };

            let listed = match &wanted {
                Wanted::Emulator(avd) => link.emulator(avd),
                // A device was asked for by its address and is listed by its
                // serial number once it answers.
                Wanted::Address(address) => link.server.devices().with(|devices| {
                    let answered = |one: &&protocol::Device| {
                        one.last.is_some()
                            && one.routes.iter().any(|route| route.address == *address)
                    };

                    devices.iter().find(answered).cloned()
                }),
            }
            .map(|device| device.id);

            if let Some(id) = listed
                && link.loaded.get()
            {
                ui.wanted.set(None);
                router.pick(Some(id));
            }
        });

        router
    }

    pub(crate) fn device(self) -> Option<DeviceId> {
        self.device.get()
    }

    pub(crate) fn page(self) -> Page {
        self.page.get()
    }

    pub(crate) fn page_untracked(self) -> Page {
        self.page.get_untracked()
    }

    pub(crate) fn on<T>(self, read: impl FnOnce(&Page) -> T) -> T {
        self.page.with(read)
    }

    /// `read` gives None for any page but the view's own. The view is still
    /// up for a moment while another page opens, so the memo keeps the
    /// position it had until the view is taken down.
    pub(crate) fn held<T>(self, read: impl Fn(Page) -> Option<T> + Send + Sync + 'static) -> Memo<T>
    where
        T: Clone + PartialEq + Send + Sync + 'static,
    {
        Memo::new(move |before: Option<&T>| {
            read(self.page.get())
                .or_else(|| before.cloned())
                .expect("a view is only built while its own page is open")
        })
    }

    /// Nothing is shut until tetherd has sent its state, so that the dock
    /// holds still while the page loads.
    pub(crate) fn shut(self, needed: Views) -> bool {
        self.loaded.get()
            && !self
                .offered
                .get()
                .is_some_and(|views| views.contains(needed))
    }

    pub(crate) fn go(self, page: Page) {
        self.stored.update(|route| route.page = page);
    }

    pub(crate) fn pick(self, device: Option<DeviceId>) {
        let another = self.device.get_untracked() != device;

        self.stored.update(|route| {
            if another {
                route.page = route.page.restarted();
            }

            route.device = device;
        });
    }

    pub(crate) fn open(self, device: DeviceId, page: Page) {
        self.stored.set(Route {
            device: Some(device),
            page,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emulator() -> DeviceId {
        DeviceId::Emulator {
            port: 5554,
            instance: protocol::InstanceId("SHA256:a/b+c=".to_owned()),
        }
    }

    #[test]
    fn every_page_comes_back_from_its_address() {
        let some = |text: &str| Some(text.to_owned());
        let pages = [
            Page::devices(),
            Page::Devices {
                emulator: some("Pixel 9/API 35"),
            },
            Page::registry(),
            Page::Registry {
                app: some("com.example.app"),
            },
            Page::files(),
            Page::Files {
                folder: "/".to_owned(),
            },
            Page::Files {
                folder: "/sdcard/My Photos/100% done".to_owned(),
            },
            Page::database(),
            Page::Database {
                app: some("com.example.app"),
                store: None,
                table: None,
            },
            Page::Database {
                app: some("com.example.app"),
                store: some("databases/main.db"),
                table: None,
            },
            Page::Database {
                app: some("com.example.app"),
                store: some("databases/main.db"),
                table: some("users/2024"),
            },
            Page::Apps,
            Page::Logs,
            Page::Crashes,
            Page::Shell,
            Page::Preferences,
            Page::Media,
            Page::Clipboard,
            Page::Camera,
            Page::Location,
            Page::Settings,
            Page::Access,
            Page::Network,
            Page::Events,
        ];

        for page in pages {
            for device in [
                None,
                Some(emulator()),
                Some(DeviceId::Android("R58M".to_owned())),
            ] {
                let route = Route {
                    device,
                    page: page.clone(),
                };

                assert_eq!(Route::from_address(&route.address()), route);
            }
        }
    }

    #[test]
    fn the_device_is_encoded_so_the_address_parts_at_the_first_slash() {
        let route = Route {
            device: Some(emulator()),
            page: Page::Files {
                folder: "/sdcard/My Photos".to_owned(),
            },
        };

        assert_eq!(
            route.address(),
            "emulator:5554@SHA256:a%2Fb%2Bc%3D/files/sdcard/My%20Photos"
        );
    }

    #[test]
    fn an_address_that_names_less_opens_the_devices() {
        let nothing = Route {
            device: None,
            page: Page::devices(),
        };

        assert_eq!(Route::from_address(""), nothing);
        assert_eq!(Route::from_address("/nothing"), nothing);
        assert_eq!(
            Route::from_address("android:R58M"),
            Route {
                device: Some(DeviceId::Android("R58M".to_owned())),
                page: Page::devices(),
            }
        );
        assert_eq!(
            Route::from_address("android:R58M/files"),
            Route {
                device: Some(DeviceId::Android("R58M".to_owned())),
                page: Page::files(),
            }
        );
    }
}
