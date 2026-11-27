//! Nothing here is specific to Android's tools, so the same page can show
//! other kinds of device.
//!
//! The page first shows tetherd its key over HTTP ([`HELLO`], [`CONNECT`]),
//! and only a trusted page is given a WebRTC connection. A message is read in
//! place with [`access`], which matters for those that carry a frame or a
//! file.

use workspace_hack as _;

pub use rkyv::{Archive, Archived};

mod apps;
mod camera;
mod events;
mod handshake;
mod messages;
mod pack;
mod state;
mod table;
mod wire;

pub use apps::*;
pub use camera::*;
pub use events::*;
pub use handshake::*;
pub use messages::*;
pub use pack::*;
pub use state::*;
pub use table::*;
pub use wire::*;
