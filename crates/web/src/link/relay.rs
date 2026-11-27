//! Each channel to a service on the device is a DataChannel of its own on the
//! link's connection (`protocol::adb_label`), and tetherd passes its bytes on
//! unchanged. A [`Relay`] is a libadb `Session` over those channels, so
//! `adb::Device` runs on it as on a direct connection.
//!
//! Writes wait while a lot is still buffered, because a DataChannel's `send`
//! fails once the browser's send buffer is full.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

use adb::Device;
use adb::libadb::channel::{ChannelId, SelectResult};
use adb::libadb::{Error, Feature, ProtocolError, Session};
use leptos::prelude::*;
use protocol::{DeviceId, PART, access, own};
use web_sys::{RtcDataChannel, RtcDataChannelState, RtcDataChannelType};

use super::heard::{Heard, bytes, ends};
use super::usb::Phone;
use super::{Link, ServerStoreFields};

/// A write waits while more than `HIGH` bytes are buffered on its channel,
/// until the buffer drains below `LOW`.
const HIGH: u32 = 1024 * 1024;
const LOW: u32 = 256 * 1024;

impl Link {
    /// It is the same device each time, because it keeps dropped channels
    /// until they are closed. A relayed device advertises no features until
    /// [`Link::adb`] has learned them.
    fn phone(self, id: &DeviceId) -> Device<Phone> {
        if let Some(held) = self.held(&id.serial()) {
            return held;
        }

        self.relays(|relays| {
            relays
                .devices
                .borrow_mut()
                .entry(id.clone())
                .or_insert_with(|| {
                    Device::new(Phone::Relay(Box::new(Relay {
                        link: self,
                        device: id.to_string(),
                        features: Rc::default(),
                    })))
                })
                .clone()
        })
    }

    /// The page only sees the banner of a phone on its own WebUSB. For one
    /// tetherd relays, the features are derived from its Android version.
    pub(crate) async fn adb(self, id: &DeviceId) -> Device<Phone> {
        let device = self.phone(id);

        if let Phone::Relay(relay) = device.session()
            && relay.features.borrow().is_none()
        {
            let sdk = android::props::property(&device, "ro.build.version.sdk").await;

            // A level that does not parse leaves the features unknown, so
            // the next call asks again.
            if let Ok(sdk) = sdk.trim().parse() {
                relay.features.replace(Some(features(sdk)));
            }
        }

        device
    }

    pub(crate) async fn rooted(self, id: &DeviceId) -> bool {
        let known = self.relays(|relays| relays.rooted.borrow().get(id).copied());

        if let Some(known) = known {
            return known;
        }

        let rooted = android::props::rooted(&self.phone(id)).await;

        self.relays(|relays| relays.rooted.borrow_mut().insert(id.clone(), rooted));

        rooted
    }

    /// Drops the cached state of each device that is not connected, so one
    /// that connects again is asked afresh.
    pub(super) fn keep(self) {
        let connected: Vec<DeviceId> = self.server.devices().with_untracked(|devices| {
            devices
                .iter()
                .filter(|device| device.connected().is_some())
                .map(|device| device.id.clone())
                .collect()
        });

        self.relays(|relays| {
            relays
                .devices
                .borrow_mut()
                .retain(|id, _| connected.contains(id));
            relays
                .rooted
                .borrow_mut()
                .retain(|id, _| connected.contains(id));
        });
    }
}

/// tetherd advertises libadb's defaults. Each feature is dated to the release
/// whose adbd had it, or a later one where that is unsure, because a missing
/// feature only falls back to an older protocol.
// shortcut: guessed from the SDK level, because the banner stays on tetherd.
// Ask tetherd for it if a device ever lacks a feature its release should have.
fn features(sdk: u32) -> Vec<Feature> {
    [
        (24, Feature::ShellV2),
        (24, Feature::Cmd),
        (26, Feature::StatV2),
        (30, Feature::Abb),
        (30, Feature::AbbExec),
        (30, Feature::LsV2),
        (30, Feature::SendrecvV2),
        (31, Feature::TrackApp),
        (34, Feature::AppInfo),
    ]
    .into_iter()
    .filter(|(since, _)| sdk >= *since)
    .map(|(_, feature)| feature)
    .collect()
}

#[derive(Clone)]
pub(crate) struct Relay {
    link: Link,
    /// The device's id as the channel label carries it.
    device: String,
    features: Rc<RefCell<Option<Vec<Feature>>>>,
}

#[derive(Default)]
pub(crate) struct Relays {
    channels: RefCell<Channels>,
    devices: RefCell<HashMap<DeviceId, Device<Phone>>>,
    rooted: RefCell<HashMap<DeviceId, bool>>,
}

pub(crate) struct Failed(String);

impl Failed {
    pub(crate) fn new(why: String) -> Self {
        Self(why)
    }
}

impl std::fmt::Display for Failed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::fmt::Debug for Failed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

type Result<T> = std::result::Result<T, Error<Failed>>;

#[derive(Default)]
struct Channels {
    next: u32,
    open: HashMap<u32, Relayed>,
}

struct Relayed {
    channel: RtcDataChannel,
    _heard: Heard,
    /// None until tetherd reports how opening it went.
    opened: Option<std::result::Result<(), String>>,
    came: VecDeque<u8>,
    closed: bool,
    /// True when it closed because the connection to tetherd dropped.
    lost: bool,
    reading: Option<Waker>,
    writing: Option<Waker>,
}

impl Relayed {
    fn wake(&mut self) -> impl Iterator<Item = Waker> + use<> {
        self.reading.take().into_iter().chain(self.writing.take())
    }
}

impl Drop for Relayed {
    fn drop(&mut self) {
        self.channel.close();
    }
}

impl Relays {
    fn with<T>(
        &self,
        channel: u32,
        then: impl FnOnce(Option<&mut Relayed>, &mut Vec<Waker>) -> T,
    ) -> T {
        let mut woken = Vec::new();
        let done = then(
            self.channels.borrow_mut().open.get_mut(&channel),
            &mut woken,
        );

        woken.into_iter().for_each(Waker::wake);

        done
    }

    /// The first message is the result of opening the channel, and the rest
    /// are the device's bytes.
    fn came(&self, channel: u32, data: Vec<u8>) {
        self.with(channel, |relayed, woken| {
            // The page closed this channel already.
            let Some(relayed) = relayed else {
                return;
            };

            if relayed.opened.is_none() {
                relayed.opened = Some(
                    access::<std::result::Result<(), String>>(&data)
                        .map_err(|error| format!("relay failed: {error}"))
                        .and_then(own::<std::result::Result<(), String>>),
                );
            } else {
                relayed.came.extend(data);
            }

            woken.extend(relayed.reading.take());
        });
    }

    /// tetherd ended a channel. It leaves closing the DataChannel to the page
    /// (`Hub::close` in crates/server/src/rtc.rs).
    fn ended(&self, channel: u32) {
        self.with(channel, |relayed, woken| {
            if let Some(relayed) = relayed {
                relayed.channel.close();
                relayed.closed = true;
                woken.extend(relayed.wake());
            }
        });
    }

    fn closed(&self, channel: u32) {
        self.with(channel, |relayed, woken| {
            if let Some(relayed) = relayed {
                relayed.closed = true;
                woken.extend(relayed.wake());
            }
        });
    }

    fn drained(&self, channel: u32) {
        self.with(channel, |relayed, woken| {
            woken.extend(relayed.and_then(|relayed| relayed.writing.take()));
        });
    }

    pub(crate) fn gone(&self) {
        // The devices kept dropped channels to close, and those went with
        // the connection.
        self.devices.borrow_mut().clear();
        self.rooted.borrow_mut().clear();

        let woken: Vec<Waker> = self
            .channels
            .borrow_mut()
            .open
            .values_mut()
            .flat_map(|relayed| {
                relayed.closed = true;
                relayed.lost = true;
                relayed.wake()
            })
            .collect();

        woken.into_iter().for_each(Waker::wake);
    }
}

fn lost(why: &str) -> Error<Failed> {
    Error::Io(Failed(why.to_owned()))
}

impl Relay {
    fn with<T>(
        &self,
        channel: u32,
        then: impl FnOnce(Option<&mut Relayed>, &mut Vec<Waker>) -> T,
    ) -> T {
        self.link.relays(|relays| relays.with(channel, then))
    }

    /// A read on a closed channel fails the way libadb's own does, which ends
    /// its loops.
    fn take(&self, channel: u32, into: &mut [u8], waker: &Waker) -> Poll<Result<usize>> {
        self.with(channel, |relayed, _| {
            let Some(relayed) = relayed else {
                return Poll::Ready(Err(Error::ChannelClosed));
            };

            if relayed.came.is_empty() {
                if relayed.lost {
                    return Poll::Ready(Err(lost("connection to the server lost")));
                }

                if relayed.closed {
                    return Poll::Ready(Err(Error::ChannelClosed));
                }

                relayed.reading = Some(waker.clone());

                return Poll::Pending;
            }

            let got = into.len().min(relayed.came.len());

            for (into, byte) in into.iter_mut().zip(relayed.came.drain(..got)) {
                *into = byte;
            }

            Poll::Ready(Ok(got))
        })
    }
}

/// Guards a channel being opened. One that failed, or was dropped while it
/// waited, is closed on both sides, because tetherd may have opened it.
struct Opening(Link, u32);

impl Drop for Opening {
    fn drop(&mut self) {
        let Opening(link, channel) = *self;

        link.relays(|relays| relays.channels.borrow_mut().open.remove(&channel));
    }
}

impl Session for Relay {
    type IoError = Failed;

    fn advertises(&self, feature: &Feature) -> bool {
        self.features
            .borrow()
            .as_ref()
            .is_some_and(|features| features.contains(feature))
    }

    async fn open_channel(&mut self, destination: &[u8]) -> Result<ChannelId> {
        let service = destination.strip_suffix(b"\0").unwrap_or(destination);
        let service = std::str::from_utf8(service)
            .map_err(|_| Error::Protocol(ProtocolError::InvalidDestination))?;

        let Some(pc) = self.link.peer() else {
            return Err(lost("not connected to the server"));
        };

        let link = self.link;
        let channel = link.relays(|relays| {
            let mut channels = relays.channels.borrow_mut();
            let channel = channels.next;

            channels.next = channel.wrapping_add(1);

            let opened = pc.create_data_channel(&protocol::adb_label(&self.device, service));

            opened.set_binary_type(RtcDataChannelType::Arraybuffer);
            opened.set_buffered_amount_low_threshold(LOW);

            let mut heard = Heard::new(&opened);

            heard.on("message", move |event| {
                if ends(&event) {
                    link.relays(|relays| relays.ended(channel));
                } else if let Some(data) = bytes(&event) {
                    link.relays(|relays| relays.came(channel, data));
                }
            });

            // tetherd's first message may arrive before the browser reports
            // the channel open, and nothing can be sent until it does.
            heard.on("open", move |_| {
                link.relays(|relays| relays.drained(channel))
            });

            heard.on("close", move |_| {
                link.relays(|relays| relays.closed(channel))
            });

            heard.on("bufferedamountlow", move |_| {
                link.relays(|relays| relays.drained(channel));
            });

            channels.open.insert(
                channel,
                Relayed {
                    channel: opened,
                    _heard: heard,
                    opened: None,
                    came: VecDeque::new(),
                    closed: false,
                    lost: false,
                    reading: None,
                    writing: None,
                },
            );

            channel
        });

        let opening = Opening(link, channel);

        poll_fn(|context| {
            self.with(channel, |relayed, _| match relayed {
                Some(Relayed {
                    opened: Some(Err(said)),
                    ..
                }) => Poll::Ready(Err(lost(said.strip_prefix("adb failed: ").unwrap_or(said)))),
                // A service that ended at once may have ended before the
                // browser reported the channel open. Its reads return what came.
                Some(Relayed {
                    opened: Some(Ok(())),
                    channel,
                    closed,
                    ..
                }) if *closed || channel.ready_state() == RtcDataChannelState::Open => {
                    Poll::Ready(Ok(()))
                }
                Some(relayed) if !relayed.closed => {
                    // Woken by tetherd's first message or by the channel
                    // opening (`drained`).
                    relayed.reading = Some(context.waker().clone());
                    relayed.writing = Some(context.waker().clone());

                    Poll::Pending
                }
                Some(relayed) if !relayed.lost => {
                    Poll::Ready(Err(lost("channel closed before it opened")))
                }
                _ => Poll::Ready(Err(lost("connection to the server lost"))),
            })
        })
        .await?;

        std::mem::forget(opening);

        Ok(ChannelId::from_raw(channel as usize, channel))
    }

    async fn read_channel(&mut self, channel: ChannelId, into: &mut [u8]) -> Result<usize> {
        poll_fn(|context| self.take(channel.local_id(), into, context.waker())).await
    }

    async fn write_channel(&mut self, channel: ChannelId, data: &[u8]) -> Result<()> {
        let channel = channel.local_id();

        poll_fn(|context| {
            self.with(channel, |relayed, _| {
                let Some(relayed) = relayed.filter(|relayed| !relayed.closed) else {
                    return Poll::Ready(Err(Error::ChannelClosed));
                };

                // `bufferedamountlow` wakes it, because its threshold is `LOW`.
                if relayed.channel.buffered_amount() > HIGH {
                    relayed.writing = Some(context.waker().clone());

                    return Poll::Pending;
                }

                let sent = data
                    .chunks(PART)
                    .all(|piece| relayed.channel.send_with_u8_array(piece).is_ok());

                // A channel the device closed before the close arrived here
                // takes nothing more. That counts as the channel closing
                // unless the connection itself dropped.
                if !sent {
                    let error = if relayed.lost {
                        lost("connection to the server lost")
                    } else {
                        Error::ChannelClosed
                    };

                    return Poll::Ready(Err(error));
                }

                Poll::Ready(Ok(()))
            })
        })
        .await?;

        Ok(())
    }

    async fn select_channel<F: Future>(
        &mut self,
        channel: ChannelId,
        into: &mut [u8],
        interrupt: F,
    ) -> Result<SelectResult<F::Output>> {
        let mut interrupt = std::pin::pin!(interrupt);

        poll_fn(
            |context| match self.take(channel.local_id(), into, context.waker()) {
                Poll::Ready(got) => Poll::Ready(got.map(SelectResult::Data)),
                Poll::Pending => interrupt
                    .as_mut()
                    .poll(context)
                    .map(|output| Ok(SelectResult::Interrupted(output))),
            },
        )
        .await
    }

    async fn close_channel(&mut self, channel: ChannelId) -> Result<()> {
        let channel = channel.local_id();
        let woken: Vec<Waker> = self.link.relays(|relays| {
            relays
                .channels
                .borrow_mut()
                .open
                .remove(&channel)
                .map(|mut relayed| relayed.wake().collect())
                .unwrap_or_default()
        });

        // A waiting read ends, since the channel is gone. Dropping it closed
        // its DataChannel.
        woken.into_iter().for_each(Waker::wake);

        Ok(())
    }
}
