//! A USB stack implements [`Inward`] and [`Outward`] over the usbmux
//! interface's bulk endpoints, as tetherd does with nusb, and [`carry`] runs a
//! mux over them. The stack opens the phone itself. The phone starts in a
//! configuration without usbmux, and has to be switched to the one with
//! [`USBMUX`] first, as usbmuxd does.

use std::future::Future;

use anyhow::Result;
use futures_channel::mpsc;
use futures_util::StreamExt;
use futures_util::future::{self, Either};

use crate::usbmux::Mux;

pub const APPLE: u16 = 0x05ac;

/// The product ids usbmuxd takes as phones and tablets that speak usbmux.
pub const PHONES: std::ops::RangeInclusive<u16> = 0x1290..=0x12af;

/// The usbmux interface's class, subclass and protocol.
pub const USBMUX: (u8, u8, u8) = (0xff, 0xfe, 0x02);

pub trait Inward {
    /// One transfer from the phone. Fails once the phone is gone.
    fn read(&mut self) -> impl Future<Output = Result<Vec<u8>>>;
}

pub trait Outward {
    /// One transfer to the phone, of exactly `data`.
    fn write(&mut self, data: Vec<u8>) -> impl Future<Output = Result<()>>;

    fn packet_size(&self) -> usize;
}

pub async fn carry(
    mux: Mux,
    mut packets: mpsc::UnboundedReceiver<Vec<u8>>,
    mut inward: impl Inward,
    mut outward: impl Outward,
) {
    let reading = async {
        while let Ok(data) = inward.read().await {
            mux.receive(&data);
        }
    };
    let writing = async {
        let size = outward.packet_size().max(1);

        while let Some(packet) = packets.next().await {
            // A transfer that fills its last packet is ended by an empty
            // one, or the phone waits for more.
            let ends = packet.len().is_multiple_of(size);

            if outward.write(packet).await.is_err() {
                break;
            }

            if ends && outward.write(Vec::new()).await.is_err() {
                break;
            }
        }
    };

    match future::select(Box::pin(reading), Box::pin(writing)).await {
        Either::Left(_) | Either::Right(_) => mux.close(),
    }
}
