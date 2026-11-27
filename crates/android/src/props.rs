use adb::Device;
use adb::libadb::Session;

pub async fn property<S>(device: &Device<S>, name: &str) -> String
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    device
        .sh(["getprop", name])
        .await
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Description {
    /// Empty on an emulator, which has none of its own.
    pub serial: String,
    pub model: String,
    pub release: String,
    pub abis: Vec<String>,
    pub rooted: bool,
}

/// None while the device is still booting, since scrcpy and installs fail
/// before `sys.boot_completed`.
pub async fn describe<S>(device: &Device<S>) -> Option<Description>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    if property(device, "sys.boot_completed").await != "1" {
        return None;
    }

    Some(Description {
        serial: property(device, "ro.serialno").await,
        model: property(device, "ro.product.model").await,
        release: property(device, "ro.build.version.release").await,
        abis: property(device, "ro.product.cpu.abilist")
            .await
            .split(',')
            .filter(|abi| !abi.is_empty())
            .map(str::to_owned)
            .collect(),
        rooted: rooted(device).await,
    })
}

pub async fn rooted<S>(device: &Device<S>) -> bool
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    device
        .script("su 0 id -u 2>/dev/null")
        .await
        .is_ok_and(|said| said.trim() == "0")
}
