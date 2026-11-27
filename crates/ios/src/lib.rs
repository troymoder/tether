use workspace_hack as _;

// Apple's signing server is asked over HTTP, which only tetherd does.
#[cfg(not(target_family = "wasm"))]
pub mod ddi;
pub mod display;
pub mod input;
pub mod lockdown;
pub mod perform;
pub mod phone;
pub mod pipe;
pub mod usb;
pub mod usbmux;

pub use idevice;
