//! A camera's sensor lies on its side in the phone, so its picture starts a
//! quarter turn off, and it turns again as the phone does.

use std::time::Duration;

use android::scrcpy::Facing;
use leptos::prelude::*;
use stylance::classes;

use super::mirror::Left;
use super::style;
use crate::app::{Icon, Picked, icons};
use crate::device::unsupported;
use crate::ui::button;
use crate::ui::loadable::Loadable;
use crate::ui::poll::use_poll;
use crate::ui::turn_tools::TurnTools;

/// The time between asking the phone how its screen is turned. The screen's
/// own reading is only kept while its video plays, so the view asks for
/// itself while it is open.
const TURN_EVERY: Duration = Duration::from_secs(1);

/// Turns are in degrees clockwise.
fn turned(turn: u16, by: i32) -> u16 {
    (i32::from(turn) + by).rem_euclid(360) as u16
}

/// Locked, portrait and landscape each keep their own turn (phone upright,
/// and turned a quarter to the left), and the picture follows the phone as
/// its screen turns, upside down too. Unlocked, there is one turn and the
/// picture stays as it is.
#[derive(Clone, Copy)]
pub(super) struct Turning {
    left: Left,
    usual: u16,
    /// The phone's screen turns counter-clockwise as the phone does. A back
    /// camera's picture turns as far the other way (-1), and a front
    /// camera, which faces the other way, turns the same way (1). A camera
    /// that is neither does not turn with the phone (0).
    with_phone: i32,
    phone_turn: Signal<Loadable<u16>>,
}

impl Turning {
    pub(super) fn new(left: Left, usual: u16, facing: Facing) -> Self {
        let device = expect_context::<Picked>().device;
        let with_phone = match facing {
            Facing::Back => -1,
            Facing::Front => 1,
            Facing::External | Facing::Unknown => 0,
        };
        let phone_turn = use_poll(TURN_EVERY, move || {
            let device = device.get();

            async move {
                let turned = device.turned().await;

                turned
                    .map(|turned| turned.degrees)
                    .ok_or_else(|| unsupported("rotation"))
            }
        });

        Turning {
            left,
            usual,
            with_phone,
            phone_turn,
        }
    }

    fn turn(self) -> u16 {
        self.left.turn.get().unwrap_or(self.usual)
    }

    pub(super) fn locked(self) -> bool {
        self.left.locked.get().unwrap_or(true)
    }

    /// A phone that has not said yet, or cannot, is taken to stand upright.
    fn rotation(self) -> i32 {
        self.phone_turn.get().ready().map_or(0, i32::from)
    }

    pub(super) fn landscape(self) -> bool {
        self.rotation() % 180 == 90
    }

    /// The phone's turn away from the orientation the kept turn is for:
    /// none, or upside down.
    fn followed(self) -> i32 {
        let level = if self.landscape() { 90 } else { 0 };

        self.with_phone * (self.rotation() - level)
    }

    /// The current orientation's own turn. Without one it is the other
    /// orientation's, turned a quarter as the phone is. Before either it
    /// is the single turn, which counts as the portrait one.
    fn kept_here(self) -> u16 {
        let by_portrait = self.left.by_portrait.get().flatten();
        let by_landscape = self.left.by_landscape.get().flatten();
        let quarter = self.with_phone * 90;

        if self.landscape() {
            by_landscape
                .or_else(|| by_portrait.map(|turn| turned(turn, quarter)))
                .unwrap_or_else(|| turned(self.turn(), quarter))
        } else {
            by_portrait
                .or_else(|| by_landscape.map(|turn| turned(turn, -quarter)))
                .unwrap_or_else(|| self.turn())
        }
    }

    pub(super) fn shown(self) -> u16 {
        if self.locked() {
            turned(self.kept_here(), self.followed())
        } else {
            self.turn()
        }
    }

    fn keep_here(self, to: u16) {
        let to = Some(turned(to, -untrack(|| self.followed())));

        if untrack(|| self.landscape()) {
            self.left.by_landscape.set(to);
        } else {
            self.left.by_portrait.set(to);
        }
    }

    fn put(self, to: u16) {
        if untrack(|| self.locked()) {
            self.keep_here(to);
        } else {
            self.left.turn.set(to);
        }
    }

    /// Locking keeps the current turn for the orientation the phone is in.
    fn toggle_lock(self) {
        let locked = untrack(|| self.locked());

        if !locked {
            self.keep_here(untrack(|| self.turn()));
        }

        self.left.locked.set(!locked);
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn MirrorTools(
    turning: Turning,
    torch: RwSignal<bool>,
    has_torch: bool,
    #[prop(into)] live: Signal<bool>,
    #[prop(into)] on_torch: Callback<bool>,
) -> impl IntoView {
    let torch_tip = move || {
        if !has_torch {
            "This camera has no torch"
        } else if torch.get() {
            "Torch off"
        } else {
            "Torch on"
        }
    };
    let lock_tip = move || {
        let orientation = if turning.landscape() {
            "landscape"
        } else {
            "portrait"
        };

        if turning.locked() {
            format!("Turn kept for {orientation}; the picture follows the device")
        } else {
            format!("Keep this turn for {orientation} only")
        }
    };

    view! {
        <div class=style::tools>
            <TurnTools
                turn=Signal::derive(move || turning.shown())
                on_turn=move |to: u16| turning.put(to)
            />
            <button
                type="button"
                role="switch"
                class=classes!(button::style::icon, style::lock)
                aria-checked=move || turning.locked().to_string()
                title=lock_tip
                aria-label=lock_tip
                on:click=move |_| turning.toggle_lock()
            >
                {move || {
                    let paths = if turning.locked() { icons::LOCKED } else { icons::UNLOCKED };

                    view! { <Icon paths /> }
                }}
                <span class=style::orientation>
                    {move || if turning.landscape() { "Landscape" } else { "Portrait" }}
                </span>
            </button>
            <span class=style::rule aria-hidden="true"></span>
            <button
                type="button"
                role="switch"
                class=classes!(button::style::icon, style::torch)
                aria-checked=move || torch.get().to_string()
                title=torch_tip
                aria-label=torch_tip
                disabled=move || !has_torch || !live.get()
                on:click=move |_| {
                    let on = !torch.get_untracked();

                    torch.set(on);
                    on_torch.run(on);
                }
            >
                <Icon paths=icons::TORCH />
            </button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_wraps_around() {
        assert_eq!(turned(270, 180), 90);
        assert_eq!(turned(0, -90), 270);
    }
}
