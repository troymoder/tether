//! The page speaks adb to these phones over WebUSB with no tetherd in
//! between. It reports every such phone to the primary tetherd, which lists
//! it among its devices like any other.

use std::collections::HashMap;

use adb::Device;
use adb::libadb::channel::{ChannelId, SelectResult};
use adb::libadb::{Error, Feature, Session};
use leptos::prelude::*;
use leptos::task::spawn_local;
use libadb::auth::Authenticator;
use libadb::keys::rsa::rand_core::{CryptoRng, RngCore};
use libadb::keys::{AdbKey, KeyName};
use libadb::split::Writer;
use libadb::{Connection, ConnectionConfig, HostBanner};
use protocol::{
    At, Described, DeviceId, DeviceState, Means, Request, ToServer, Topic, UsbPhone, Way,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Usb, UsbConnectionEvent, UsbDevice, UsbDeviceFilter, UsbDeviceRequestOptions};

use self::transport::Transport;
use super::Link;
use super::heard::Heard;
use super::relay::{Failed, Relay};

mod registry;
mod transport;

/// The name phones list this browser's adb key under.
const KEY_NAME: &str = "tether@browser";

#[derive(Clone)]
pub(crate) enum Phone {
    /// Boxed, because a relay carries the whole link.
    Relay(Box<Relay>),
    Usb(Writer<Transport>),
}

#[derive(Default)]
pub(crate) struct Phones {
    held: HashMap<String, Held>,
    /// The plugged-in phones the browser allowed before, by serial. Each is
    /// reported as available until it is taken.
    plugged: HashMap<String, UsbDevice>,
    _heard: Option<Heard>,
}

/// A phone from the moment it is opened, so letting go of it during the
/// handshake also closes it.
struct Held {
    /// None until the handshake is done and the primary tetherd agreed.
    device: Option<Device<Phone>>,
    usb: UsbDevice,
}

/// The browser's random source, for libadb's RSA blinding.
struct Random;

impl RngCore for Random {
    fn next_u32(&mut self) -> u32 {
        libadb::keys::rsa::rand_core::impls::next_u32_via_fill(self)
    }

    fn next_u64(&mut self) -> u64 {
        libadb::keys::rsa::rand_core::impls::next_u64_via_fill(self)
    }

    fn fill_bytes(&mut self, into: &mut [u8]) {
        if let Ok(crypto) = window().crypto() {
            let _ = crypto.get_random_values_with_u8_array(into);
        }
    }

    fn try_fill_bytes(
        &mut self,
        into: &mut [u8],
    ) -> Result<(), libadb::keys::rsa::rand_core::Error> {
        self.fill_bytes(into);

        Ok(())
    }
}

impl CryptoRng for Random {}

/// This browser's key, which calls `asking` when the phone has to ask its
/// owner to allow it.
struct Asking<'a, F: Fn()> {
    key: &'a mut AdbKey,
    asking: &'a F,
}

impl<F: Fn()> Authenticator for Asking<'_, F> {
    type Error = <AdbKey as Authenticator>::Error;

    fn sign(&mut self, token: &[u8]) -> impl Future<Output = Result<Vec<u8>, Self::Error>> {
        self.key.sign(token)
    }

    fn public_key(&self) -> &[u8] {
        self.key.public_key()
    }

    fn offer_public_key(&self) -> bool {
        (self.asking)();

        true
    }
}

fn navigator_usb() -> Usb {
    window().navigator().usb()
}

fn id_of(usb: &UsbDevice) -> String {
    usb.serial_number()
        .unwrap_or_else(|| format!("usb-{:04x}-{:04x}", usb.vendor_id(), usb.product_id()))
}

fn reported(id: &str, name: &str, held: Option<DeviceState>) -> UsbPhone {
    UsbPhone {
        id: id.to_owned(),
        name: name.to_owned(),
        held,
        described: None,
    }
}

impl Link {
    pub(crate) fn held(self, id: &str) -> Option<Device<Phone>> {
        self.phones
            .with_value(|phones| phones.held.get(id).and_then(|held| held.device.clone()))
    }

    fn own_usb(self) -> Option<Way> {
        Some(Way {
            means: Means::Usb,
            at: At::Browser(self.you.get_untracked()?),
        })
    }

    pub(super) fn report_usb(self) {
        self.send(&ToServer::Usb(self.usb.get_untracked()));
    }

    /// A phone is only taken when the user asks, never because the browser
    /// allowed it before, so tetherd can have it whenever this page does not.
    /// The browser only reports phones it allowed.
    pub(crate) fn watch_usb(self) {
        let usb = navigator_usb();
        let mut heard = Heard::new(&usb);

        heard.on("connect", move |event| {
            if let Some(event) = event.dyn_ref::<UsbConnectionEvent>() {
                self.plug(event.device());
            }
        });
        heard.on("disconnect", move |event| {
            if let Some(event) = event.dyn_ref::<UsbConnectionEvent>() {
                let id = id_of(&event.device());

                self.phones.update_value(|phones| {
                    phones.plugged.remove(&id);
                });
                self.let_go(&id);
            }
        });
        self.phones
            .update_value(|phones| phones._heard = Some(heard));

        Effect::new(move |_| {
            self.usb.track();
            self.report_usb();
        });

        // A phone held here installs each new build of an app with
        // auto-install on, so the apps topic is followed while one is held.
        let holds = Memo::new(move |_| {
            self.usb
                .with(|usb| usb.iter().any(|phone| phone.held.is_some()))
        });

        self.follow(move || holds.get().then_some(Topic::Apps));
        self.auto_install();

        spawn_local(async move {
            let Ok(allowed) = JsFuture::from(usb.get_devices()).await else {
                return;
            };

            for device in js_sys::Array::from(&allowed) {
                self.plug(device.unchecked_into());
            }
        });
    }

    fn plug(self, usb: UsbDevice) {
        let id = id_of(&usb);
        let name = usb.product_name().unwrap_or_else(|| id.clone());

        self.phones.update_value(|phones| {
            phones.plugged.insert(id.clone(), usb);
        });

        if self
            .usb
            .with_untracked(|usb| usb.iter().all(|phone| phone.id != id))
        {
            self.usb.update(|usb| usb.push(reported(&id, &name, None)));
        }
    }

    /// With `take`, the primary tetherd lets go of the phone wherever else it
    /// is connected.
    pub(crate) fn use_usb(self, id: &str, take: bool) {
        let usb = self
            .phones
            .with_value(|phones| phones.plugged.get(id).cloned());

        if let Some(usb) = usb {
            spawn_local(self.take(usb, take));
        }
    }

    /// The chooser only opens from a user gesture.
    pub(crate) fn choose_usb(self) {
        let filter = UsbDeviceFilter::new();

        filter.set_class_code(0xff);
        filter.set_subclass_code(0x42);
        filter.set_protocol_code(0x01);

        let options = UsbDeviceRequestOptions::new(&[filter]);
        let asked = navigator_usb().request_device(&options);

        spawn_local(async move {
            // Closing the chooser is not an error.
            if let Ok(device) = JsFuture::from(asked).await {
                let device: UsbDevice = device.unchecked_into();

                self.plug(device.clone());
                self.take(device, false).await;
            }
        });
    }

    /// It fails while another program such as tetherd holds the phone, and
    /// the primary tetherd refuses it while the phone is connected elsewhere,
    /// unless `take`.
    async fn take(self, usb: UsbDevice, take: bool) {
        let id = id_of(&usb);
        let taken = self
            .phones
            .with_value(|phones| phones.held.contains_key(&id));

        if taken {
            return;
        }

        let name = usb.product_name().unwrap_or_else(|| id.clone());

        self.phones.update_value(|phones| {
            phones.held.insert(
                id.clone(),
                Held {
                    device: None,
                    usb: usb.clone(),
                },
            );
        });

        let result = self.handshake(&usb, &id, &name).await;
        let device = match result {
            Ok(device) => device,
            Err(error) => {
                self.let_go(&id);
                self.say(format!("{error:?}"));
                return;
            }
        };

        let Some(way) = self.own_usb() else {
            self.let_go(&id);
            self.say("usb failed: not connected to the server");
            return;
        };

        // The handshake gave the serial, which the primary tetherd knows the
        // phone by, and the primary is asked before the phone is used here.
        // It only connects through a route it was told of, so the report goes
        // first on the same ordered channel.
        self.report_usb();

        let asked = Request::Connect {
            device: DeviceId::Android(id.clone()),
            way: Some(way),
            code: None,
            take,
        };

        if let Err(error) = self.call(asked).await {
            self.let_go(&id);
            self.say(error);
            return;
        }

        // A disconnect may have released it meanwhile, which also ended the
        // handshake.
        let still = self.phones.try_update_value(|phones| {
            phones
                .held
                .get_mut(&id)
                .map(|held| held.device = Some(device.clone()))
                .is_some()
        });

        if still == Some(true) {
            self.describe(&id, &name, &device).await;
        }
    }

    /// It waits while the phone asks its owner, and reports the phone as
    /// unauthorized meanwhile.
    async fn handshake(
        self,
        usb: &UsbDevice,
        id: &str,
        name: &str,
    ) -> Result<Device<Phone>, Failed> {
        let transport = Transport::open(usb).await?;
        let pkcs8 = super::key::adb_key()
            .await
            .ok_or_else(|| Failed::new("key failed: no WebCrypto".to_owned()))?;
        let key_name =
            KeyName::new(KEY_NAME).map_err(|error| Failed::new(format!("key failed: {error}")))?;
        let mut key = AdbKey::from_pkcs8_der(&pkcs8, &mut Random, key_name)
            .map_err(|error| Failed::new(format!("key failed: {error}")))?;
        let asking = || self.list(reported(id, name, Some(DeviceState::Unauthorized)));
        let auth = Asking {
            key: &mut key,
            asking: &asking,
        };
        // A screen, a shell, logs and polls each hold channels at once, as
        // they do through tetherd.
        let config = ConnectionConfig::new().with_max_channels(256);
        let connection = Connection::connect(transport, auth, HostBanner::default(), config)
            .await
            .map_err(|error| Failed::new(format!("adb failed: {error}")))?;
        let (reader, writer) = connection
            .split()
            .map_err(|error| Failed::new(format!("adb failed: {error}")))?;
        let gone = id.to_owned();

        spawn_local(async move {
            let _ = reader.run().await;

            self.let_go(&gone);
        });

        Ok(Device::new(Phone::Usb(writer)))
    }

    /// A phone that cannot describe itself is still booting, so it is
    /// reported as offline.
    async fn describe(self, id: &str, name: &str, device: &Device<Phone>) {
        let Some(said) = android::props::describe(device).await else {
            self.list(reported(id, name, Some(DeviceState::Offline)));
            return;
        };

        let name = if said.model.is_empty() {
            name.to_owned()
        } else {
            said.model
        };

        self.list(UsbPhone {
            id: id.to_owned(),
            name,
            held: Some(DeviceState::Ready),
            described: Some(Described {
                release: said.release,
                abis: said.abis,
            }),
        });
    }

    /// The phone stays reported while it is plugged in.
    pub(crate) fn let_go(self, id: &str) {
        let held = self
            .phones
            .try_update_value(|phones| phones.held.remove(id))
            .flatten();

        if let Some(held) = held {
            // Closing aborts the reads in flight and releases the interface.
            spawn_local(async move {
                if held.usb.opened()
                    && let Err(error) = JsFuture::from(held.usb.close()).await
                {
                    self.say(format!("usb failed: {error:?}"));
                }
            });
        }

        let plugged = self
            .phones
            .try_with_value(|phones| phones.plugged.get(id).cloned())
            .flatten();

        self.usb.try_update(|usb| {
            usb.retain(|phone| phone.id != id);

            if let Some(plugged) = plugged {
                let name = plugged.product_name().unwrap_or_else(|| id.to_owned());

                usb.push(reported(id, &name, None));
            }
        });
    }

    /// Plugged in or not, each with its name and id.
    pub(crate) async fn allowed_usb(self) -> Vec<(String, String, UsbDevice)> {
        let Ok(allowed) = JsFuture::from(navigator_usb().get_devices()).await else {
            return Vec::new();
        };

        js_sys::Array::from(&allowed)
            .into_iter()
            .map(|device| {
                let device: UsbDevice = device.unchecked_into();
                let id = id_of(&device);

                (
                    device.product_name().unwrap_or_else(|| id.clone()),
                    id,
                    device,
                )
            })
            .collect()
    }

    pub(crate) async fn forget_usb(self, device: UsbDevice) {
        let id = id_of(&device);

        self.phones.update_value(|phones| {
            phones.plugged.remove(&id);
        });
        self.let_go(&id);

        if let Err(error) = JsFuture::from(device.forget()).await {
            self.say(format!("usb failed: {error:?}"));
        }
    }

    fn list(self, phone: UsbPhone) {
        self.usb.update(|usb| {
            usb.retain(|listed| listed.id != phone.id);
            usb.push(phone);
        });
    }
}

impl Session for Phone {
    type IoError = Failed;

    fn advertises(&self, feature: &Feature) -> bool {
        match self {
            Phone::Relay(relay) => relay.advertises(feature),
            Phone::Usb(writer) => writer.advertises(feature),
        }
    }

    async fn open_channel(&mut self, destination: &[u8]) -> Result<ChannelId, Error<Failed>> {
        match self {
            Phone::Relay(relay) => relay.open_channel(destination).await,
            Phone::Usb(writer) => writer.open_channel(destination).await,
        }
    }

    async fn read_channel(
        &mut self,
        channel: ChannelId,
        into: &mut [u8],
    ) -> Result<usize, Error<Failed>> {
        match self {
            Phone::Relay(relay) => relay.read_channel(channel, into).await,
            Phone::Usb(writer) => writer.read_channel(channel, into).await,
        }
    }

    async fn write_channel(
        &mut self,
        channel: ChannelId,
        data: &[u8],
    ) -> Result<(), Error<Failed>> {
        match self {
            Phone::Relay(relay) => relay.write_channel(channel, data).await,
            Phone::Usb(writer) => writer.write_channel(channel, data).await,
        }
    }

    async fn select_channel<F: Future>(
        &mut self,
        channel: ChannelId,
        into: &mut [u8],
        interrupt: F,
    ) -> Result<SelectResult<F::Output>, Error<Failed>> {
        match self {
            Phone::Relay(relay) => relay.select_channel(channel, into, interrupt).await,
            Phone::Usb(writer) => writer.select_channel(channel, into, interrupt).await,
        }
    }

    async fn close_channel(&mut self, channel: ChannelId) -> Result<(), Error<Failed>> {
        match self {
            Phone::Relay(relay) => relay.close_channel(channel).await,
            Phone::Usb(writer) => writer.close_channel(channel).await,
        }
    }
}
