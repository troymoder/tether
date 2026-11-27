use embedded_io::{ErrorKind, ErrorType};
use embedded_io_async::{Read, Write};
use js_sys::Uint8Array;
use libadb::Splittable;
use libadb::transport::ReadCancelSafety;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    UsbAlternateInterface, UsbDevice, UsbDirection, UsbEndpoint, UsbEndpointType,
    UsbInTransferResult, UsbInterface, UsbTransferStatus,
};

use crate::link::relay::Failed;

/// The class, subclass and protocol of an adb interface.
const ADB: (u8, u8, u8) = (0xff, 0x42, 0x01);

impl embedded_io::Error for Failed {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

/// Clones share the device, so one can read while another writes.
#[derive(Clone)]
pub(crate) struct Transport {
    device: UsbDevice,
    inward: u8,
    outward: u8,
    /// The OUT endpoint's packet size.
    packet: usize,
    /// True once the phone's first handshake reply arrived. A phone closed
    /// in the middle of a session still has that session's writes queued.
    /// Those arrive first and are dropped until then, as Tango does
    /// (ya-webadb adb-daemon-webusb device.ts).
    synced: bool,
}

/// True if `data` is one whole message header answering a handshake: CNXN,
/// AUTH or STLS, with its magic.
fn handshake_reply(data: &[u8]) -> bool {
    let Ok(header) = <[u8; 24]>::try_from(data) else {
        return false;
    };
    let word = |at: usize| {
        u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
    };
    let command = word(0);

    matches!(command, 0x4e58_4e43 | 0x4854_5541 | 0x534c_5453) && word(20) == !command
}

fn failed(error: &wasm_bindgen::JsValue) -> Failed {
    let said = error
        .dyn_ref::<js_sys::Error>()
        .map(|error| String::from(error.message()))
        .unwrap_or_else(|| format!("{error:?}"));

    Failed::new(format!("usb failed: {said}"))
}

impl Transport {
    /// Claiming fails while another program such as tetherd or an adb server
    /// holds the interface.
    pub(super) async fn open(device: &UsbDevice) -> Result<Self, Failed> {
        JsFuture::from(device.open())
            .await
            .map_err(|error| failed(&error))?;

        if device.configuration().is_none() {
            JsFuture::from(device.select_configuration(1))
                .await
                .map_err(|error| failed(&error))?;
        }

        let configuration = device
            .configuration()
            .ok_or_else(|| Failed::new("usb failed: no configuration".to_owned()))?;

        for interface in configuration.interfaces() {
            let interface: UsbInterface = interface.unchecked_into();

            for alternate in interface.alternates() {
                let alternate: UsbAlternateInterface = alternate.unchecked_into();
                let kind = (
                    alternate.interface_class(),
                    alternate.interface_subclass(),
                    alternate.interface_protocol(),
                );

                if kind != ADB {
                    continue;
                }

                let number = interface.interface_number();

                JsFuture::from(device.claim_interface(number))
                    .await
                    .map_err(|error| failed(&error))?;

                if alternate.alternate_setting() != interface.alternate().alternate_setting() {
                    JsFuture::from(
                        device.select_alternate_interface(number, alternate.alternate_setting()),
                    )
                    .await
                    .map_err(|error| failed(&error))?;
                }

                return Self::on(device.clone(), &alternate);
            }
        }

        Err(Failed::new("usb failed: no adb interface".to_owned()))
    }

    fn on(device: UsbDevice, alternate: &UsbAlternateInterface) -> Result<Self, Failed> {
        let mut inward = None;
        let mut outward = None;

        for endpoint in alternate.endpoints() {
            let endpoint: UsbEndpoint = endpoint.unchecked_into();

            if endpoint.type_() != UsbEndpointType::Bulk {
                continue;
            }

            match endpoint.direction() {
                UsbDirection::In => inward = Some(endpoint.endpoint_number()),
                UsbDirection::Out => {
                    outward = Some((endpoint.endpoint_number(), endpoint.packet_size()));
                }
                _ => {}
            }
        }

        let (Some(inward), Some((outward, packet))) = (inward, outward) else {
            return Err(Failed::new("usb failed: no bulk endpoints".to_owned()));
        };

        Ok(Self {
            device,
            inward,
            outward,
            packet: packet as usize,
            synced: false,
        })
    }

    async fn send(&self, data: &[u8]) -> Result<(), Failed> {
        let sending = self
            .device
            .transfer_out_with_u8_slice(self.outward, data)
            .map_err(|error| failed(&error))?;

        JsFuture::from(sending)
            .await
            .map_err(|error| failed(&error))?;

        Ok(())
    }
}

impl ErrorType for Transport {
    type Error = Failed;
}

impl Read for Transport {
    async fn read(&mut self, into: &mut [u8]) -> Result<usize, Failed> {
        loop {
            let asked = self.device.transfer_in(self.inward, into.len() as u32);
            let result: UsbInTransferResult = JsFuture::from(asked)
                .await
                .map_err(|error| failed(&error))?
                .unchecked_into();

            if result.status() != UsbTransferStatus::Ok {
                return Err(Failed::new(format!("usb failed: {:?}", result.status())));
            }

            let Some(data) = result.data() else {
                return Ok(0);
            };
            let data = Uint8Array::new_with_byte_offset_and_length(
                &data.buffer(),
                data.byte_offset() as u32,
                data.byte_length() as u32,
            );
            let got = (data.length() as usize).min(into.len());

            data.subarray(0, got as u32).copy_to(&mut into[..got]);

            if !self.synced {
                if !handshake_reply(&into[..got]) {
                    continue;
                }

                self.synced = true;
            }

            return Ok(got);
        }
    }
}

impl Write for Transport {
    /// Sends `data` as one transfer and, when it fills its last packet, a
    /// zero-length one after it. adbd reads in 16 KiB requests that only a
    /// short packet ends early, so without it the bytes wait on the phone
    /// for the next write.
    async fn write(&mut self, data: &[u8]) -> Result<usize, Failed> {
        if data.is_empty() {
            return Ok(0);
        }

        self.send(data).await?;

        if data.len().is_multiple_of(self.packet) {
            self.send(&[]).await?;
        }

        Ok(data.len())
    }

    async fn flush(&mut self) -> Result<(), Failed> {
        Ok(())
    }
}

/// A transfer handed to the device cannot be cancelled, so dropping a read
/// loses the bytes the device already put in it.
impl ReadCancelSafety for Transport {
    fn read_cancel_safe(&self) -> bool {
        false
    }
}

impl Splittable for Transport {
    type ReadHalf = Transport;
    type WriteHalf = Transport;

    fn split(self) -> Result<(Transport, Transport), Failed> {
        let writing = self.clone();

        Ok((self, writing))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(command: u32, magic: u32) -> Vec<u8> {
        let mut header = command.to_le_bytes().to_vec();

        header.resize(20, 0);
        header.extend(magic.to_le_bytes());
        header
    }

    #[test]
    fn knows_a_handshake_reply_from_what_a_closed_session_left() {
        assert!(handshake_reply(&header(0x4e58_4e43, !0x4e58_4e43)));
        assert!(handshake_reply(&header(0x4854_5541, !0x4854_5541)));
        // WRTE, a stale header with its magic.
        assert!(!handshake_reply(&header(0x4554_5257, !0x4554_5257)));
        assert!(!handshake_reply(&header(0x4e58_4e43, 0)));
        assert!(!handshake_reply(&[0, 0, 0, 2]));
    }
}
