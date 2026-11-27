use std::future::Future;

use anyhow::{Context, Result, anyhow, bail};
use ios::usb::{APPLE, Inward, Outward, PHONES, USBMUX};
use ios::usbmux::Mux;
use nusb::Interface;
use nusb::descriptors::Configuration;
use nusb::transfer::{Direction, EndpointType, Queue, RequestBuffer};

const READS: usize = 4;
const READ_SIZE: usize = 64 * 1024;

/// An iPhone's UDID is its USB serial number.
pub(super) fn on_cables() -> Vec<String> {
    let Ok(found) = nusb::list_devices() else {
        return Vec::new();
    };

    found
        .filter(|device| device.vendor_id() == APPLE && PHONES.contains(&device.product_id()))
        .filter_map(|device| device.serial_number().map(str::to_owned))
        .collect()
}

struct Place {
    configuration: u8,
    interface: u8,
    alternate: u8,
    inward: u8,
    outward: u8,
    packet: usize,
}

fn usbmux_in(configuration: &Configuration<'_>) -> Option<Place> {
    let setting = configuration
        .interface_alt_settings()
        .find(|setting| (setting.class(), setting.subclass(), setting.protocol()) == USBMUX)?;
    let bulk = |direction| {
        setting.endpoints().find(|endpoint| {
            endpoint.transfer_type() == EndpointType::Bulk && endpoint.direction() == direction
        })
    };
    let inward = bulk(Direction::In)?;
    let outward = bulk(Direction::Out)?;

    Some(Place {
        configuration: configuration.configuration_value(),
        interface: setting.interface_number(),
        alternate: setting.alternate_setting(),
        inward: inward.address(),
        outward: outward.address(),
        packet: outward.max_packet_size(),
    })
}

struct Reads(Queue<RequestBuffer>);

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

struct Writes {
    interface: Interface,
    endpoint: u8,
    packet: usize,
}

impl Outward for Writes {
    async fn write(&mut self, data: Vec<u8>) -> Result<()> {
        let written = self.interface.bulk_out(self.endpoint, data).await;

        written
            .status
            .map_err(|error| anyhow!("usb failed: {error}"))
    }

    fn packet_size(&self) -> usize {
        self.packet
    }
}

/// The returned future carries the mux's traffic until the phone is unplugged
/// and has to be spawned before `Mux::start`.
pub(super) fn open(udid: &str) -> Result<(Mux, impl Future<Output = ()> + Send + 'static)> {
    let info = nusb::list_devices()?
        .find(|device| device.vendor_id() == APPLE && device.serial_number() == Some(udid))
        .with_context(|| format!("usb failed: {} not plugged in", super::mention(udid)))?;
    let device = info.open().context("usb failed")?;
    let active = device.active_configuration().ok();
    // The active configuration's usbmux first: while the screen is captured
    // that is QuickTime's, which also has one, and moving off it would end
    // the capture.
    let Some(place) = active.as_ref().and_then(usbmux_in).or_else(|| {
        device
            .configurations()
            .find_map(|configuration| usbmux_in(&configuration))
    }) else {
        bail!(
            "usb failed: {} has no usbmux interface",
            super::mention(udid)
        );
    };

    if active.as_ref().map(Configuration::configuration_value) != Some(place.configuration) {
        // The kernel's drivers hold the interfaces of the configuration it
        // is in, which stops it being changed.
        if let Some(active) = &active {
            for interface in active.interfaces() {
                let _ = device.detach_kernel_driver(interface.interface_number());
            }
        }

        device
            .set_configuration(place.configuration)
            .context("usb failed: switching to usbmux")?;
    }

    let interface = device
        .detach_and_claim_interface(place.interface)
        .context("usb failed: claiming usbmux")?;

    if place.alternate != 0 {
        interface
            .set_alt_setting(place.alternate)
            .context("usb failed")?;
    }

    let mut reads = interface.bulk_in_queue(place.inward);

    for _ in 0..READS {
        reads.submit(RequestBuffer::new(READ_SIZE));
    }

    let writes = Writes {
        interface,
        endpoint: place.outward,
        packet: place.packet,
    };
    let (mux, packets) = Mux::new();
    let running = ios::usb::carry(mux.clone(), packets, Reads(reads), writes);

    Ok((mux, running))
}
