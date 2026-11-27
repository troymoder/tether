//! SVG path data on a 24 by 24 grid, drawn as lines unless noted.

use leptos::prelude::*;

pub(crate) const DEVICES: &str =
    r#"<path d="M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z"/>"#;
pub(crate) const SETTINGS: &str = r#"<path d="M4 7h10M18 7h2M4 17h2M10 17h10M14 5v4M6 15v4"/>"#;
pub(crate) const APPS: &str =
    r#"<path d="M12 3l8 4.5v9L12 21l-8-4.5v-9z"/><path d="M4 7.5l8 4.5 8-4.5M12 12v9"/>"#;
pub(crate) const LOGS: &str = r#"<path d="M4 6h16M4 10h16M4 14h10M4 18h13"/>"#;
pub(crate) const FOLLOW: &str =
    r#"<path d="M4 12a8 8 0 0 1 14-5.3M20 4v4h-4M20 12a8 8 0 0 1-14 5.3M4 20v-4h4"/>"#;
pub(crate) const STATS: &str = r#"<path d="M5 20V10M10 20V4M15 20v-7M20 20v-12"/>"#;
pub(crate) const CHEVRON: &str = r#"<path d="M6 9l6 6 6-6"/>"#;
pub(crate) const ADD: &str = r#"<path d="M12 5v14M5 12h14"/>"#;
pub(crate) const CLOSE: &str = r#"<path d="M6 6l12 12M18 6L6 18"/>"#;
pub(crate) const PLAY: &str = r#"<path d="M8 5l11 7-11 7z"/>"#;
pub(crate) const SQUARE: &str = r#"<path d="M6 6h12v12H6z"/>"#;
pub(crate) const SOUND: &str =
    r#"<path d="M4 10v4h3l5 4V6L7 10H4zM16 9a4 4 0 0 1 0 6M18.5 6.5a8 8 0 0 1 0 11"/>"#;
pub(crate) const FOLDER: &str = r#"<path d="M3 6h6l2 2h10v11H3z"/>"#;
pub(crate) const FILE: &str = r#"<path d="M6 3h8l4 4v14H6zM14 3v4h4"/>"#;
pub(crate) const AUDIO: &str = r#"<path d="M9 18V6l10-2v12"/><circle cx="6.5" cy="18" r="2.5"/><circle cx="16.5" cy="16" r="2.5"/>"#;
pub(crate) const UPLOAD: &str = r#"<path d="M12 16V5M7.5 9.5L12 5l4.5 4.5M5 19.5h14"/>"#;
pub(crate) const BIN: &str = r#"<path d="M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13"/>"#;
pub(crate) const DOWNLOAD: &str = r#"<path d="M12 4v11M7.5 11l4.5 4.5 4.5-4.5M5 19.5h14"/>"#;
pub(crate) const UP: &str = r#"<path d="M12 19V6M6.5 11.5L12 6l5.5 5.5"/>"#;
pub(crate) const DATABASE: &str = r#"<path d="M4 6c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"/>"#;
pub(crate) const CRASH: &str = r#"<path d="M12 4l9 16H3L12 4zM12 10v4M12 17v.5"/>"#;
pub(crate) const CLIPBOARD: &str = r#"<path d="M9 4h6v3H9zM7 5H5v16h14V5h-2M9 12h6M9 16h6"/>"#;
pub(crate) const COPY: &str = r#"<path d="M9 4h6v3H9zM9 5H5v15h14V5h-4"/>"#;
pub(crate) const IMAGE: &str = r#"<path d="M4 5h16v14H4zM4 16l5-5 4 4 3-3 4 4"/>"#;
pub(crate) const STAR: &str =
    r#"<path d="M12 3.6l2.6 5.3 5.8.8-4.2 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.2-4.1 5.8-.8z"/>"#;
pub(crate) const PIN: &str = r#"<path d="M12 21s7-6.2 7-11a7 7 0 0 0-14 0c0 4.8 7 11 7 11zM12 12.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z"/>"#;
pub(crate) const POWER: &str = r#"<path d="M12 3v9M6.3 6.8a8 8 0 1 0 11.4 0"/>"#;
pub(crate) const QUIETER: &str = r#"<path d="M4 10v4h3l5 4V6L7 10H4zM16 12h6"/>"#;
pub(crate) const LOUDER: &str = r#"<path d="M4 10v4h3l5 4V6L7 10H4zM16 12h6M19 9v6"/>"#;
pub(crate) const MUTED: &str = r#"<path d="M4 10v4h3l5 4V6L7 10H4zM16 9l5 6M21 9l-5 6"/>"#;
pub(crate) const RECORD: &str = r#"<path d="M12 19a7 7 0 1 0 0-14 7 7 0 0 0 0 14z"/>"#;
pub(crate) const BACK: &str = r#"<path d="M15 5l-7 7 7 7"/>"#;
pub(crate) const HOME: &str = r#"<path d="M4 11l8-7 8 7v9h-5v-6H9v6H4z"/>"#;
pub(crate) const RECENTS: &str = r#"<path d="M5 5h14v14H5z"/>"#;
pub(crate) const SCREENSHOT: &str =
    r#"<path d="M4 8h4l1.5-2h5L16 8h4v11H4zM12 16a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"/>"#;
pub(crate) const ROTATE: &str = r#"<path d="M7 4h10v16H7zM3 9v6M21 9v6"/>"#;
pub(crate) const CHECK: &str = r#"<path d="M5 12l5 5 9-10"/>"#;
/// Filled.
pub(crate) const APPLE: &str = r#"<path d="M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.376-2-.156-3.675 1.09-4.61 1.09zM15.53 3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 1.338.104 2.715-.688 3.559-1.701"/>"#;
/// Filled.
pub(crate) const ANDROID: &str = r#"<path d="M6 18c0 .55.45 1 1 1h1v3.5c0 .83.67 1.5 1.5 1.5s1.5-.67 1.5-1.5V19h2v3.5c0 .83.67 1.5 1.5 1.5s1.5-.67 1.5-1.5V19h1c.55 0 1-.45 1-1V8H6v10zM3.5 8C2.67 8 2 8.67 2 9.5v7c0 .83.67 1.5 1.5 1.5S5 17.33 5 16.5v-7C5 8.67 4.33 8 3.5 8zm17 0c-.83 0-1.5.67-1.5 1.5v7c0 .83.67 1.5 1.5 1.5s1.5-.67 1.5-1.5v-7c0-.83-.67-1.5-1.5-1.5zm-4.97-5.84l1.3-1.3c.2-.2.2-.51 0-.71-.2-.2-.51-.2-.71 0l-1.48 1.48C13.85 1.23 12.95 1 12 1c-.96 0-1.86.23-2.66.63L7.85.15c-.2-.2-.51-.2-.71 0-.2.2-.2.51 0 .71l1.31 1.31C6.97 3.26 6 5.01 6 7h12c0-1.99-.97-3.75-2.47-4.84zM10 5H9V4h1v1zm5 0h-1V4h1v1z"/>"#;
pub(crate) const DOWN: &str = r#"<path d="M12 5v13M6.5 12.5L12 18l5.5-5.5"/>"#;
pub(crate) const SEARCH: &str = r#"<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>"#;
pub(crate) const CLOCK: &str = r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>"#;
pub(crate) const CAMERA: &str = r#"<path d="M3 7h12v10H3zM15 10l6-3v10l-6-3"/>"#;
pub(crate) const CAMERA_LIVE: &str =
    r#"<path d="M3 7h12v10H3zM15 10l6-3v10l-6-3"/><circle cx="9" cy="12" r="2.2"/>"#;
pub(crate) const NETWORK: &str = r#"<path d="M3 17l5-6 4 3 4-7 5 5M3 21h18"/>"#;
pub(crate) const GEAR: &str = r#"<path d="M10.5 5.2L10.8 2.6L13.2 2.6L13.5 5.2L15.8 6.1L17.8 4.5L19.5 6.2L17.9 8.2L18.8 10.5L21.4 10.8L21.4 13.2L18.8 13.5L17.9 15.8L19.5 17.8L17.8 19.5L15.8 17.9L13.5 18.8L13.2 21.4L10.8 21.4L10.5 18.8L8.2 17.9L6.2 19.5L4.5 17.8L6.1 15.8L5.2 13.5L2.6 13.2L2.6 10.8L5.2 10.5L6.1 8.2L4.5 6.2L6.2 4.5L8.2 6.1zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"/>"#;
pub(crate) const DEVICE: &str =
    r#"<path d="M8 3h8a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM11 18h2"/>"#;
pub(crate) const DEVICE_OFF: &str = r#"<path d="M8 3h8a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM11 18h2M4 4l16 16"/>"#;
pub(crate) const TOKEN: &str = r#"<path d="M13 11L4 20M7 17l2 2"/><circle cx="16" cy="8" r="4"/>"#;
pub(crate) const RIGHT: &str = r#"<path d="M9 6l6 6-6 6"/>"#;
pub(crate) const WIFI: &str =
    r#"<path d="M5 12.5a10 10 0 0 1 14 0M2 9a15 15 0 0 1 20 0M8.5 16a5 5 0 0 1 7 0M12 19.5h.01"/>"#;
pub(crate) const USB: &str =
    r#"<path d="M12 3v14M9 6l3-3 3 3M7 10v3l5 2M17 9v3l-5 2"/><circle cx="12" cy="19" r="2"/>"#;
pub(crate) const PAUSE: &str = r#"<path d="M8 5v14M16 5v14"/>"#;
pub(crate) const LAYERS: &str = r#"<path d="M4 7l8-4 8 4-8 4-8-4zM4 12l8 4 8-4M4 17l8 4 8-4"/>"#;
pub(crate) const STORAGE: &str = r#"<path d="M4 5h16v14H4zM4 14h16M8 17h.5"/>"#;
pub(crate) const SIDE: &str = r#"<path d="M4 5h16v14H4zM9 5v14"/>"#;
pub(crate) const TURN_LEFT: &str = r#"<path d="M4 12a8 8 0 1 0 2.6-5.9M4 4v5h5"/>"#;
pub(crate) const TURN_RIGHT: &str = r#"<path d="M20 12a8 8 0 1 1-2.6-5.9M20 4v5h-5"/>"#;
pub(crate) const MIRROR: &str = r#"<path d="M12 3v18M8 7l-5 5 5 5zM16 7l5 5-5 5z"/>"#;
pub(crate) const FRAME: &str = r#"<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/>"#;
pub(crate) const TORCH: &str = r#"<path d="M9 3h6l-1 6h3l-6 12 1-8H8z"/>"#;
pub(crate) const LOCKED: &str = r#"<path d="M6 11h12v9H6zM9 11V8a3 3 0 0 1 6 0v3"/>"#;
pub(crate) const UNLOCKED: &str = r#"<path d="M6 11h12v9H6zM9 11V8a3 3 0 0 1 5.8-1"/>"#;
pub(crate) const CENTRE: &str =
    r#"<circle cx="12" cy="12" r="5"/><path d="M12 3v4M12 17v4M3 12h4M17 12h4"/>"#;
pub(crate) const LESS: &str = r#"<path d="M5 12h14"/>"#;
pub(crate) const LIFT: &str = r#"<path d="M6 15l6-6 6 6"/>"#;
pub(crate) const FILTER: &str = r#"<path d="M4 4.5h16l-6 7v6l-4 2v-8z"/>"#;
pub(crate) const SHELL: &str = r#"<path d="M4 5h16v14H4zM7 9l3 3-3 3M12 15h5"/>"#;
pub(crate) const FOLD: &str = r#"<path d="M4 5h16v14H4zM9 5v14M15 10l-2 2 2 2"/>"#;
pub(crate) const UNFOLD: &str = r#"<path d="M4 5h16v14H4zM9 5v14M13 10l2 2-2 2"/>"#;
pub(crate) const SUN: &str = r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>"#;
pub(crate) const MOON: &str = r#"<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>"#;

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Icon(paths: &'static str) -> impl IntoView {
    view! { <svg viewBox="0 0 24 24" aria-hidden="true" inner_html=paths></svg> }
}
