//! An iPhone's screen and sound the way QuickTime on a Mac records them over
//! USB, on any iOS since 8 and without Developer Mode, while the phone's
//! camera keeps working.
//!
//! The phone only answers while it is unlocked (iOS 27.3). The protocol is
//! qvh's (github.com/danielpaulus/quicktime_video_hack, MIT).

use workspace_hack as _;

mod black;
mod capture;
mod session;
mod sps;
mod usb;
mod video;
mod wire;

pub use capture::{Capture, Media, STARTING, WARM, cycle, ready, release};
pub use usb::Phone;
