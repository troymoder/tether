//! A log can be long and fast, so its lines are not page elements. The rows
//! in view are drawn on a terminal (ui/xterm.rs) and redrawn as it scrolls.
//!
//! The view (view.rs) is built from parts that share three handles: the two
//! logs and their marks (panes.rs), the filters (sift.rs), and whether the
//! log follows its end (reading.rs).

mod device;
mod device_log;
mod filter_popup;
mod find;
mod frame;
pub(crate) mod front;
mod line;
mod line_detail;
mod log;
mod output_log;
mod panes;
mod reading;
mod rows;
mod sift;
mod tick;
mod view;

pub(crate) use device::{Lines, Logs, Sources};
pub(crate) use view::Logs;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "logs.module.scss"
);
