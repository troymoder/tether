//! The page. Everything it does goes through tetherd over one WebRTC
//! connection (link.rs).

use wasm_bindgen::JsCast;
use workspace_hack as _;

mod app;
mod cache;
mod device;
mod link;
mod ui;
mod util;
mod views;

fn main() {
    console_error_panic_hook::set_once();

    // The camera's worker (link/capture.rs) loads this same module and calls
    // link::worker::capture_worker instead, since it has no page to mount on.
    if js_sys::global().is_instance_of::<web_sys::Window>() {
        leptos::mount::mount_to_body(app::App);
    }
}
