//! tetherd holds the pair record of the phones on its USB and performs
//! [`IosOperation`]s for the page. The page builds everything else on the
//! streams and values they give, so the key stays with tetherd.

use anyhow::{Context, Result, anyhow};
use idevice::lockdown::LockdownClient;
use idevice::pairing_file::PairingFile;
use idevice::provider::IdeviceProvider;
use idevice::{IdeviceError, ReadWrite};
use protocol::IosOperation;

use crate::lockdown::PORT;
use crate::phone::{Phone, Ports};

pub enum Done {
    /// A service's stream, with its TLS done.
    Stream(Box<dyn ReadWrite>),
    /// A binary plist.
    Value(Vec<u8>),
    Nothing,
}

fn failed(error: IdeviceError) -> anyhow::Error {
    anyhow!("lockdown failed: {error}")
}

/// The bool is true if the phone needs the TLS of iOS before 5.
async fn session<P: Ports>(phone: &Phone<P>) -> Result<(LockdownClient, PairingFile, bool)> {
    let record = phone.get_pairing_file().await.map_err(failed)?;
    let mut lockdown = LockdownClient::new(phone.connect(PORT).await.map_err(failed)?);
    let legacy = lockdown.start_session(&record).await.map_err(failed)?;

    Ok((lockdown, record, legacy))
}

pub async fn perform<P: Ports>(phone: &Phone<P>, operation: &IosOperation) -> Result<Done> {
    let (mut lockdown, record, legacy) = session(phone).await?;

    match operation {
        IosOperation::Open { service } => {
            let (port, tls) = lockdown.start_service(service).await.map_err(failed)?;
            let mut opened = phone.connect(port).await.map_err(failed)?;

            if tls {
                opened
                    .start_session(&record, legacy)
                    .await
                    .map_err(failed)?;
            }

            let stream = opened
                .get_socket()
                .with_context(|| format!("lockdown failed: {service} gave no stream"))?;

            Ok(Done::Stream(stream))
        }
        IosOperation::Value { domain, key } => {
            let value = lockdown
                .get_value(key.as_deref(), domain.as_deref())
                .await
                .map_err(failed)?;
            let mut written = Vec::new();

            plist::to_writer_binary(&mut written, &value).context("plist failed")?;

            Ok(Done::Value(written))
        }
        IosOperation::SetValue { domain, key, value } => {
            let value = plist::from_bytes(value).context("lockdown failed: not a plist")?;

            lockdown
                .set_value(key, value, domain.as_deref())
                .await
                .map_err(failed)?;

            Ok(Done::Nothing)
        }
        IosOperation::RemoveValue { domain, key } => {
            lockdown
                .remove_value(key, domain.as_deref())
                .await
                .map_err(failed)?;

            Ok(Done::Nothing)
        }
    }
}
