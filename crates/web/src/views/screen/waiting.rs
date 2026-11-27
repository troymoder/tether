use protocol::{Condition, Device, Status};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Waiting {
    NotConnected,
    InUse,
    Off,
    Booting,
    Stopping,
    /// It is connected and its video is on its way.
    Connecting,
}

pub(super) fn resting(device: &Device) -> (Waiting, String) {
    let how = match (device.emulator.as_ref(), &device.condition) {
        (Some(run), _) => match run.status {
            Status::Shutdown => Waiting::Off,
            Status::Booting => Waiting::Booting,
            Status::Stopping => Waiting::Stopping,
            Status::Booted => Waiting::Connecting,
        },
        (None, Condition::Connected { .. }) => Waiting::Connecting,
        (None, Condition::InUse) => Waiting::InUse,
        (None, Condition::Available | Condition::Unavailable) => Waiting::NotConnected,
    };

    (how, device.name.clone())
}
