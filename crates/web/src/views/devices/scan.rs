//! Pairing by a QR code the phone scans. The code names an mDNS service and
//! gives a password. tetherd waits on a `Media::Pair` lane of the code's own
//! for the phone to announce itself under that name. Dropping a code closes
//! its lane, which ends tetherd's wait for it.

use std::time::Duration;

use futures_util::StreamExt;
use leptos::prelude::*;
use leptos::task::spawn_local;
use protocol::{Media, Pairing, access, own};

use super::style;
use crate::app::Global;
use crate::link::lane::Lane;

const LIFE: f64 = 120_000.0;
/// The time codes keep replacing themselves after the last one asked for by hand.
const RENEWING: f64 = 600_000.0;
const FLASH: f64 = 2_000.0;

#[derive(Clone, PartialEq)]
enum Card {
    Live { made: f64, renewed: Option<f64> },
    Expired,
    Scanned { address: String, connecting: bool },
}

#[derive(Clone, PartialEq)]
struct Code {
    name: String,
    password: String,
    drawn: Option<Drawn>,
}

impl Code {
    fn new() -> Self {
        let name = format!("tether-{}", random(6));
        let password = random(10);
        // Android's format for the Wireless debugging QR code.
        let drawn = drawn(&format!("WIFI:T:ADB;S:{name};P:{password};;"));

        Code {
            name,
            password,
            drawn,
        }
    }
}

/// `side` counts a module of white margin on each edge.
#[derive(Clone, PartialEq)]
struct Drawn {
    side: f64,
    tiles: String,
    eyes: String,
    cores: String,
}

/// Draws `text` at error correction H, which the mark in the middle needs.
fn drawn(text: &str) -> Option<Drawn> {
    let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::H).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    let side = width as f64 + 2.0;
    let middle = side / 2.0;
    let finder = |x: usize, y: usize| (x < 7 || x >= width - 7) && y < 7 || x < 7 && y >= width - 7;
    let under_mark = |x: f64, y: f64| (x - middle).abs() < 4.0 && (y - middle).abs() < 4.0;
    let mut tiles = String::new();

    for (at, color) in colors.iter().enumerate() {
        let x = at % width;
        let y = at / width;
        // Each tile's corner in the drawing, its margin included.
        let left = x as f64 + 1.0;
        let top = y as f64 + 1.0;

        if *color == qrcode::Color::Dark && !finder(x, y) && !under_mark(left + 0.5, top + 0.5) {
            tiles.push_str(&format!("M{:.2} {:.2}h.3v.3h-.3z", left + 0.35, top + 0.35));
        }
    }

    let mut eyes = String::new();
    let mut cores = String::new();

    for (x, y) in [(1.0, 1.0), (side - 8.0, 1.0), (1.0, side - 8.0)] {
        // A 7 by 7 ring with a 5 by 5 hole, cut out by the even-odd rule.
        eyes.push_str(&format!(
            "M{} {y}h2.6q2.2 0 2.2 2.2v2.6q0 2.2 -2.2 2.2h-2.6q-2.2 0 -2.2 -2.2v-2.6q0 -2.2 2.2 -2.2z",
            x + 2.2
        ));
        eyes.push_str(&format!(
            "M{} {}v2.2q0 1.4 1.4 1.4h2.2q1.4 0 1.4 -1.4v-2.2q0 -1.4 -1.4 -1.4h-2.2q-1.4 0 -1.4 1.4z",
            x + 1.0,
            y + 2.4
        ));
        cores.push_str(&format!(
            "M{} {}h1q1 0 1 1v1q0 1 -1 1h-1q-1 0 -1 -1v-1q0 -1 1 -1z",
            x + 3.0,
            y + 2.0
        ));
    }

    Some(Drawn {
        side,
        tiles,
        eyes,
        cores,
    })
}

/// The letters leave out the ones that are easy to confuse (l, o, 0, 1).
fn random(count: usize) -> String {
    const LETTERS: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";

    (0..count)
        .map(|_| {
            let at = (js_sys::Math::random() * LETTERS.len() as f64) as usize;

            char::from(LETTERS[at.min(LETTERS.len() - 1)])
        })
        .collect()
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Scan() -> impl IntoView {
    let Global { link, ui, router } = expect_context::<Global>();
    let code = RwSignal::new(Code::new());
    let card = RwSignal::new(Card::Live {
        made: js_sys::Date::now(),
        renewed: None,
    });
    let now = RwSignal::new(js_sys::Date::now());
    let asked = StoredValue::new(js_sys::Date::now());
    // Replacing the lane closes the one before, and so does the popup
    // closing.
    let lane = StoredValue::new_local(None::<Lane>);
    let wait = move |shown: &Code| {
        let Some(opened) = link.lane(&Media::Pair {
            name: shown.name.clone(),
            password: shown.password.clone(),
        }) else {
            lane.set_value(None);
            return;
        };

        let mut told = opened.heard();
        let name = shown.name.clone();

        lane.set_value(Some(opened));

        spawn_local(async move {
            while let Some(message) = told.next().await {
                // A step for a code since replaced is not this card's.
                if code.try_with_untracked(|code| code.name != name) != Some(false) {
                    return;
                }

                let Ok(step) = access::<Pairing>(&message).map(own::<Pairing>) else {
                    continue;
                };

                match step {
                    Pairing::Scanned { address } => card.set(Card::Scanned {
                        address,
                        connecting: false,
                    }),
                    Pairing::Connecting => card.update(|card| {
                        if let Card::Scanned { connecting, .. } = card {
                            *connecting = true;
                        }
                    }),
                    Pairing::Connected { device } => {
                        ui.adding.set(None);
                        router.pick(Some(device));
                    }
                    Pairing::Paired => {
                        let address = card.with_untracked(|card| match card {
                            Card::Scanned { address, .. } => address.clone(),
                            _ => String::new(),
                        });

                        ui.adding.set(None);
                        link.say(format!("Paired with {address}"));
                    }
                    Pairing::Failed(why) => {
                        link.say(why);
                        card.set(Card::Expired);
                        lane.set_value(None);
                    }
                }
            }
        });
    };

    let renew = move |by_hand: bool| {
        let made = js_sys::Date::now();
        let fresh = Code::new();

        if by_hand {
            asked.set_value(made);
        }

        wait(&fresh);
        code.set(fresh);
        card.set(Card::Live {
            made,
            renewed: (!by_hand).then_some(made),
        });
    };

    wait(&code.get_untracked());

    if let Ok(timer) = set_interval_with_handle(
        move || {
            let at = js_sys::Date::now();

            now.set(at);

            let made = match card.try_get_untracked() {
                Some(Card::Live { made, .. }) => made,
                _ => return,
            };

            if at - made < LIFE {
                return;
            }

            if !document().hidden() && at - asked.get_value() < RENEWING {
                renew(false);
            } else {
                lane.set_value(None);
                card.set(Card::Expired);
            }
        },
        Duration::from_secs(1),
    ) {
        on_cleanup(move || timer.clear());
    }

    let covered = move || card.with(|card| !matches!(card, Card::Live { .. }));
    let foot = move || {
        let at = now.get();

        card.with(|card| match card {
            Card::Live {
                renewed: Some(renewed),
                ..
            } if at - renewed < FLASH => (None, "New code".to_owned()),
            Card::Live { made, .. } => {
                let left = ((LIFE - (at - made)) / 1000.0).ceil().max(0.0) as u32;

                (None, format!("Expires in {}:{:02}", left / 60, left % 60))
            }
            Card::Expired => (Some(style::bad), "Expired".to_owned()),
            Card::Scanned { .. } => (Some(style::ok), "Scanned".to_owned()),
        })
    };

    view! {
        <div class=style::code_card>
            <div class=style::code_tile>
                {move || {
                    code.with(|shown| shown.drawn.clone())
                        .map(|drawn| {
                            let view_box = format!("0 0 {0} {0}", drawn.side);
                            let middle = drawn.side / 2.0;

                            view! {
                                <svg
                                    role="img"
                                    aria-label="QR code for the phone to scan"
                                    viewBox=view_box
                                    class=move || covered().then_some(style::covered)
                                >
                                    <path
                                        d=drawn.tiles
                                        fill="#0f1724"
                                        stroke="#0f1724"
                                        stroke-width=".4"
                                        stroke-linejoin="round"
                                    />
                                    <path d=drawn.eyes fill="#0f1724" fill-rule="evenodd" />
                                    <path d=drawn.cores fill="#0f1724" />
                                    <rect
                                        x=middle - 3.5
                                        y=middle - 3.5
                                        width="7"
                                        height="7"
                                        rx="1.2"
                                        fill="#ffffff"
                                    />
                                    <rect
                                        x=middle - 2.8
                                        y=middle - 2.8
                                        width="5.6"
                                        height="5.6"
                                        rx="1"
                                        fill="#0f1724"
                                    />
                                    <text
                                        x=middle
                                        y=middle + 1.7
                                        text-anchor="middle"
                                        font-family="JetBrains Mono, monospace"
                                        font-weight="600"
                                        font-size="4.4"
                                        fill="#ffffff"
                                    >
                                        "t"
                                    </text>
                                </svg>
                            }
                        })
                }}
                {move || {
                    card.with(|shown| match shown {
                        Card::Live { .. } => ().into_any(),
                        Card::Expired => {
                            view! {
                                <button
                                    type="button"
                                    class=style::code_over
                                    aria-label="The code expired. Make a new code"
                                    on:click=move |_| renew(true)
                                >
                                    <span class=style::code_new>"New code"</span>
                                </button>
                            }
                                .into_any()
                        }
                        Card::Scanned { address, connecting } => {
                            let step = if *connecting { "Connecting…" } else { "Pairing…" };

                            view! {
                                <div class=style::code_over role="status">
                                    <span class=style::code_spin aria-hidden="true"></span>
                                    <strong>{address.clone()}</strong>
                                    <span>{step}</span>
                                </div>
                            }
                                .into_any()
                        }
                    })
                }}
            </div>
            {move || {
                let (mark, said) = foot();

                view! {
                    <div class=style::code_foot class=(style::bad, mark == Some(style::bad)) class=(style::ok, mark == Some(style::ok))>
                        {mark.map(|_| view! { <span class=style::mark aria-hidden="true"></span> })}
                        <span>{said}</span>
                    </div>
                }
            }}
        </div>
    }
}
