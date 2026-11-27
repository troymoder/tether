//! A vendor request makes the phone drop off the bus and come back with one
//! more configuration, which holds a QuickTime interface (class 0xff,
//! subclass 0x2a) beside usbmux. The same request with another index hides
//! it again.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use nusb::transfer::{
    Control, ControlType, Direction, EndpointType, Queue, Recipient, RequestBuffer,
};

use crate::session::{Inward, Outward};

const APPLE: u16 = 0x05ac;

const QUICKTIME: (u8, u8) = (0xff, 0x2a);

const SWITCH: u8 = 0x52;
const ON: u16 = 2;
const OFF: u16 = 0;

const READS: usize = 4;
const READ_SIZE: usize = 64 * 1024;

const COMING_BACK: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
struct Place {
    configuration: u8,
    interface: u8,
    inward: u8,
    outward: u8,
}

/// The phone's USB serial number is its UDID without the dash.
#[derive(Clone)]
pub struct Phone {
    serial: String,
}

pub(crate) struct Claimed {
    pub(crate) inward: Reads,
    pub(crate) outward: Writes,
}

impl Phone {
    pub fn new(udid: &str) -> Self {
        Self {
            serial: udid.replace('-', ""),
        }
    }

    fn open(&self) -> Result<nusb::Device> {
        let found = nusb::list_devices()?
            .find(|device| {
                device.vendor_id() == APPLE && device.serial_number() == Some(&self.serial)
            })
            .with_context(|| format!("usb failed: {} not plugged in", self.serial))?;

        found.open().context("usb failed")
    }

    pub(crate) fn shows_quicktime(&self) -> Result<bool> {
        Ok(quicktime(&self.open()?).is_some())
    }

    pub(crate) async fn switch_on(&self) -> Result<()> {
        let device = self.open()?;

        if quicktime(&device).is_some() {
            return Ok(());
        }

        switch(&device, ON);
        drop(device);

        let started = Instant::now();

        while started.elapsed() < COMING_BACK {
            tokio::time::sleep(Duration::from_millis(250)).await;

            if self.open().ok().as_ref().and_then(quicktime).is_some() {
                tracing::debug!("switched on after {:?}", started.elapsed());

                return Ok(());
            }
        }

        bail!("usb failed: the QuickTime configuration never came");
    }

    pub(crate) fn activate(&self) -> Result<()> {
        let device = self.open()?;
        let place = quicktime(&device).context("usb failed: no QuickTime configuration")?;

        activate(&device, place)
    }

    pub(crate) fn claim(&self) -> Result<Claimed> {
        let device = self.open()?;
        let place = quicktime(&device).context("usb failed: no QuickTime configuration")?;

        activate(&device, place)?;

        // Another process streaming from this phone holds the interface
        // through usbfs, which this claim never takes from it.
        let interface = device
            .detach_and_claim_interface(place.interface)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::ResourceBusy => {
                    anyhow!("usb failed: another program is streaming from this phone")
                }
                _ => anyhow!("usb failed: claiming QuickTime: {error}"),
            })?;

        interface.clear_halt(place.inward).context("usb failed")?;
        interface.clear_halt(place.outward).context("usb failed")?;

        let mut reads = interface.bulk_in_queue(place.inward);

        for _ in 0..READS {
            reads.submit(RequestBuffer::new(READ_SIZE));
        }

        Ok(Claimed {
            inward: Reads(reads),
            outward: Writes(interface, place.outward),
        })
    }

    /// Without the reset the phone keeps listing the configuration, and the
    /// next session reuses it, which the phone pings on but never streams
    /// (iOS 27.3). After the reset usbmuxd takes the phone back by itself.
    pub(crate) async fn switch_off(&self) -> Result<()> {
        let device = self.open()?;

        switch(&device, OFF);

        // The phone drops off the bus, which reads as a failure here.
        if let Err(error) = device.reset() {
            tracing::debug!("reset: {error}");
        }

        drop(device);

        let started = Instant::now();

        while started.elapsed() < COMING_BACK {
            tokio::time::sleep(Duration::from_millis(250)).await;

            if let Ok(device) = self.open()
                && quicktime(&device).is_none()
            {
                tracing::debug!("switched off after {:?}", started.elapsed());

                return Ok(());
            }
        }

        bail!("usb failed: the phone kept its QuickTime configuration");
    }
}

fn switch(device: &nusb::Device, index: u16) {
    let request = Control {
        control_type: ControlType::Vendor,
        recipient: Recipient::Device,
        request: SWITCH,
        value: 0,
        index,
    };

    // The phone may drop off the bus before it answers, which reads as a
    // failure here although the switch happened.
    if let Err(error) = device.control_out_blocking(request, &[], Duration::from_secs(1)) {
        tracing::debug!("switch {index}: {error}");
    }
}

/// Whatever holds the active configuration's interfaces, as usbmuxd holds
/// usbmux's, is detached first, since it would stop the configuration being
/// set.
fn activate(device: &nusb::Device, place: Place) -> Result<()> {
    let active = device.active_configuration().ok();

    if active
        .as_ref()
        .map(nusb::descriptors::Configuration::configuration_value)
        == Some(place.configuration)
    {
        return Ok(());
    }

    detach_all(device);
    device
        .set_configuration(place.configuration)
        .context("usb failed: switching to QuickTime")
}

fn detach_all(device: &nusb::Device) {
    if let Ok(active) = device.active_configuration() {
        for interface in active.interfaces() {
            let _ = device.detach_kernel_driver(interface.interface_number());
        }
    }
}

fn quicktime(device: &nusb::Device) -> Option<Place> {
    device.configurations().find_map(|configuration| {
        let setting = configuration
            .interface_alt_settings()
            .find(|setting| (setting.class(), setting.subclass()) == QUICKTIME)?;
        let bulk = |direction| {
            setting
                .endpoints()
                .find(|endpoint| {
                    endpoint.transfer_type() == EndpointType::Bulk
                        && endpoint.direction() == direction
                })
                .map(|endpoint| endpoint.address())
        };

        Some(Place {
            configuration: configuration.configuration_value(),
            interface: setting.interface_number(),
            inward: bulk(Direction::In)?,
            outward: bulk(Direction::Out)?,
        })
    })
}

pub(crate) struct Reads(Queue<RequestBuffer>);

impl Inward for Reads {
    async fn read(&mut self) -> Result<Vec<u8>> {
        let read = self.0.next_complete().await;

        read.status
            .map_err(|error| anyhow!("usb failed: {error}"))?;

        let data = read.data.clone();

        self.0.submit(RequestBuffer::reuse(read.data, READ_SIZE));

        Ok(data)
    }
}

pub(crate) struct Writes(nusb::Interface, u8);

impl Outward for Writes {
    async fn write(&mut self, data: Vec<u8>) -> Result<()> {
        self.0
            .bulk_out(self.1, data)
            .await
            .status
            .map_err(|error| anyhow!("usb failed: {error}"))
    }
}
