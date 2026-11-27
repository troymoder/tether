use anyhow::{Result, anyhow};
use idevice::lockdown::LockdownClient;
use idevice::pairing_file::PairingFile;
use idevice::{Idevice, IdeviceError};

use crate::phone::LABEL;
use crate::usbmux::Mux;

pub const PORT: u16 = 62078;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Description {
    pub udid: String,
    pub name: String,
    pub model: String,
    pub release: String,
}

#[derive(Debug)]
pub enum Pairing {
    /// The pair record, as a plist.
    Paired(Vec<u8>),
    Asking,
    Refused,
    Locked,
}

async fn lockdown(mux: &Mux) -> Result<LockdownClient> {
    let stream = mux.connect(PORT).await?;

    Ok(LockdownClient::new(Idevice::new(Box::new(stream), LABEL)))
}

fn failed(error: IdeviceError) -> anyhow::Error {
    anyhow!("lockdown failed: {error}")
}

pub async fn describe(mux: &Mux) -> Result<Description> {
    let mut lockdown = lockdown(mux).await?;
    let mut text = async |key: &str| -> Result<String> {
        let value = lockdown.get_value(Some(key), None).await.map_err(failed)?;

        Ok(value.as_string().unwrap_or_default().to_owned())
    };

    Ok(Description {
        udid: text("UniqueDeviceID").await?,
        name: text("DeviceName").await?,
        model: text("ProductType").await?,
        release: text("ProductVersion").await?,
    })
}

/// Asking again while the phone shows the prompt waits on the same prompt.
pub async fn pair(mux: &Mux, host: &str, buid: &str, name: &str) -> Result<Pairing> {
    let mut lockdown = lockdown(mux).await?;

    match lockdown.pair_once(host, buid, Some(name)).await {
        Ok(record) => Ok(Pairing::Paired(record.serialize().map_err(failed)?)),
        Err(IdeviceError::PairingDialogResponsePending) => Ok(Pairing::Asking),
        Err(IdeviceError::UserDeniedPairing) => Ok(Pairing::Refused),
        Err(IdeviceError::PasswordProtected) => Ok(Pairing::Locked),
        Err(error) => Err(failed(error)),
    }
}

pub async fn session(mux: &Mux, record: &[u8]) -> Result<LockdownClient> {
    let record = PairingFile::from_bytes(record).map_err(failed)?;
    let mut lockdown = lockdown(mux).await?;

    lockdown.start_session(&record).await.map_err(failed)?;

    Ok(lockdown)
}
