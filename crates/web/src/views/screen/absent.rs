//! With no device picked, the pane keeps a disabled bar over a blank screen,
//! so the page keeps its layout.

use leptos::prelude::*;
use stylance::classes;

use super::geometry::SHAPE;
use super::style;
use crate::app::{Global, icons};
use crate::ui::button::IconButton;

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Absent() -> impl IntoView {
    let link = expect_context::<Global>().link;
    let handset = format!("--width: {}; --height: {}", SHAPE.0, SHAPE.1);
    let shape = handset.clone();

    view! {
        // app.module.scss and app/drop.rs find the device's pane by this id.
        <aside id="phone" class=style::phone aria-label="Device">
            <div class=style::controls>
                <IconButton paths=icons::SCREENSHOT label="Screenshot" disabled=true />
                <IconButton paths=icons::RECORD label="Record" disabled=true />
                <IconButton paths=icons::STATS label="Video stats" disabled=true />
                <span class=style::rule aria-hidden="true"></span>
                <IconButton paths=icons::MUTED label="Play the device's sound" disabled=true />
                <input type="range" class=style::loud aria-label="Volume" disabled=true />
            </div>
            <div class=style::screen>
                <div class=style::handset style=handset>
                    <div class=style::face>
                        <div
                            class=classes!(style::surface, style::waiting)
                            // Blank until tetherd has sent its state, so it
                            // never says "No device" too early.
                            class=(style::none, move || link.loaded.get())
                            style=shape
                        >
                            <Show when=move || link.loaded.get()>
                                <p>"No device"</p>
                            </Show>
                        </div>
                    </div>
                    // The empty row of face buttons keeps the screen at the
                    // same place as a real device's.
                    <div class=style::nav_keys></div>
                </div>
            </div>
        </aside>
    }
}
