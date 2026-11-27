use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

use anyhow::Result;
use idevice::pairing_file::PairingFile;
use idevice::provider::IdeviceProvider;
use idevice::{Idevice, IdeviceError, ReadWrite};

use crate::usbmux::Mux;

pub(crate) const LABEL: &str = "tether";

pub trait Ports: Clone + Debug + Send + Sync + Unpin + 'static {
    fn connect(&self, port: u16) -> impl Future<Output = Result<Box<dyn ReadWrite>>> + Send;
}

impl Ports for Mux {
    async fn connect(&self, port: u16) -> Result<Box<dyn ReadWrite>> {
        Ok(Box::new(Mux::connect(self, port).await?))
    }
}

#[derive(Debug)]
pub struct Phone<P> {
    ports: P,
    /// The pair record, as a plist.
    record: Vec<u8>,
}

impl<P: Ports> Phone<P> {
    pub fn new(ports: P, record: Vec<u8>) -> Self {
        Self { ports, record }
    }
}

impl<P: Ports> IdeviceProvider for Phone<P> {
    fn connect(
        &self,
        port: u16,
    ) -> Pin<Box<dyn Future<Output = Result<Idevice, IdeviceError>> + Send>> {
        let ports = self.ports.clone();

        Box::pin(async move {
            let stream = ports
                .connect(port)
                .await
                .map_err(|error| IdeviceError::UnexpectedResponse(format!("{error:#}")))?;

            Ok(Idevice::new(stream, LABEL))
        })
    }

    fn label(&self) -> &str {
        LABEL
    }

    fn get_pairing_file(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<PairingFile, IdeviceError>> + Send>> {
        let record = self.record.clone();

        Box::pin(async move { PairingFile::from_bytes(&record) })
    }
}
