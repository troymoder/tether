use std::collections::VecDeque;
use std::time::Duration;

use js_sys::{Map, Reflect};
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::RtcPeerConnection;

use super::{Link, Linked};

struct Reading {
    route: String,
    /// In milliseconds.
    round_trip: Option<f64>,
    /// The share of ICE connectivity checks left unanswered. A DataChannel
    /// has no packet loss figure of its own, so this stands in for it.
    lost: Option<f64>,
    totals: Totals,
}

/// The bytes are summed over every candidate pair, because tetherd can send
/// from a pair other than the one the browser selected, whose count then stays
/// at 0. They cover the data channels and the screen's RTP tracks alike. The
/// packets are the RTP packets of every incoming track.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Totals {
    received: f64,
    sent: f64,
    packets: f64,
    lost: f64,
}

impl Totals {
    /// A total that went down, as when a track or a pair left the report,
    /// counts as zero.
    fn since(self, last: Totals) -> Totals {
        Totals {
            received: (self.received - last.received).max(0.0),
            sent: (self.sent - last.sent).max(0.0),
            packets: (self.packets - last.packets).max(0.0),
            lost: (self.lost - last.lost).max(0.0),
        }
    }

    fn loss(self) -> Option<f64> {
        let due = self.packets + self.lost;

        (due > 0.0).then(|| self.lost / due)
    }
}

pub(crate) const MINUTE: usize = 60;

/// Read from the browser each second for as long as the page is open, because
/// the graph shows the last minute as soon as the Network view opens. Each
/// series is its own signal, so a reader of one does not rerun for the others.
#[derive(Clone, Copy)]
pub(crate) struct Figures {
    /// The bytes received and sent in each second, oldest first.
    pub traffic: RwSignal<VecDeque<(u32, u32)>>,
    /// The total bytes received and sent.
    pub carried: RwSignal<(u64, u64)>,
    /// In milliseconds, oldest first.
    pub round_trips: RwSignal<VecDeque<Option<f64>>>,
    pub lost: RwSignal<Option<f64>>,
    /// The share of RTP packets lost in each second, oldest first.
    pub losses: RwSignal<VecDeque<Option<f64>>>,
}

impl Figures {
    pub(super) fn new() -> Self {
        Figures {
            traffic: RwSignal::new(VecDeque::new()),
            carried: RwSignal::new((0, 0)),
            round_trips: RwSignal::new(VecDeque::new()),
            lost: RwSignal::new(None),
            losses: RwSignal::new(VecDeque::new()),
        }
    }

    fn second(self, round_trip: Option<f64>, loss: Option<f64>, traffic: (u32, u32)) {
        self.round_trips.update(|trips| keep(trips, round_trip));
        self.losses.update(|losses| keep(losses, loss));
        self.traffic.update(|seconds| keep(seconds, traffic));
    }
}

impl Link {
    /// Reads every second, which also catches ICE moving to another candidate
    /// pair.
    pub(super) fn measure(self) {
        let link = self;
        let figures = self.figures;
        let last = StoredValue::new(None::<Totals>);
        let look = move |second: bool| {
            let Some(pc) = link.peer() else {
                last.set_value(None);
                figures.second(None, None, (0, 0));
                // The checks of a closed connection say nothing about the
                // next one.
                figures.lost.set(None);

                return;
            };

            leptos::task::spawn_local(async move {
                let reading = read(&pc).await;

                // Dropped when the connection closed during the read.
                if !link.connected.get_untracked() {
                    return;
                }

                let round_trip = reading.as_ref().and_then(|reading| reading.round_trip);
                let totals = reading.as_ref().map(|reading| reading.totals);
                let moved = match (totals, last.get_value()) {
                    (Some(totals), Some(last)) => totals.since(last),
                    _ => Totals::default(),
                };
                let down = moved.received as u32;
                let up = moved.sent as u32;

                last.set_value(totals);

                if second {
                    figures.second(round_trip, moved.loss(), (down, up));
                }

                figures.carried.update(|carried| {
                    carried.0 += u64::from(down);
                    carried.1 += u64::from(up);
                });

                figures
                    .lost
                    .set(reading.as_ref().and_then(|reading| reading.lost));

                link.relink(|linked| {
                    if let Linked::Connected { route, .. } = linked {
                        *route = reading.map(|reading| reading.route);
                    }
                });
            });
        };

        Effect::new(move |_| {
            if link.connected.get() {
                look(false);
            }
        });

        let _ = set_interval_with_handle(move || look(true), Duration::from_secs(1));
    }
}

fn keep<T>(seconds: &mut VecDeque<T>, figure: T) {
    if seconds.len() == MINUTE {
        seconds.pop_front();
    }

    seconds.push_back(figure);
}

async fn read(pc: &RtcPeerConnection) -> Option<Reading> {
    let report: Map = JsFuture::from(pc.get_stats()).await.ok()?.unchecked_into();
    let field = |stats: &JsValue, name: &str| {
        Reflect::get(stats, &name.into())
            .ok()
            .and_then(|value| value.as_string())
    };

    let mut pair = None;
    let mut totals = Totals::default();

    report.for_each(&mut |stats, _| {
        let number = |name: &str| {
            Reflect::get(&stats, &name.into())
                .ok()
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0)
        };

        match field(&stats, "type").as_deref() {
            Some("candidate-pair") => {
                totals.received += number("bytesReceived");
                totals.sent += number("bytesSent");
            }
            Some("inbound-rtp") => {
                totals.packets += number("packetsReceived");
                totals.lost += number("packetsLost");
            }
            _ => {}
        }
    });

    // Chromium names the selected pair on the transport. Firefox does not, so
    // there the nominated pair that succeeded is used.
    report.for_each(&mut |stats, _| {
        if let Some(id) = field(&stats, "selectedCandidatePairId") {
            pair = report.get(&id.into()).dyn_into().ok();
        } else if pair.is_none()
            && field(&stats, "type").as_deref() == Some("candidate-pair")
            && field(&stats, "state").as_deref() == Some("succeeded")
            && Reflect::get(&stats, &"nominated".into()).is_ok_and(|on| on.is_truthy())
        {
            pair = Some(stats);
        }
    });

    let pair = pair?;
    let candidates = ["localCandidateId", "remoteCandidateId"]
        .map(|side| field(&pair, side).map(|id| report.get(&id.into())));
    let [Some(local), Some(remote)] = candidates else {
        return None;
    };

    let either = |name, value: &str| {
        [&local, &remote]
            .iter()
            .any(|candidate| field(candidate, name).as_deref() == Some(value))
    };

    let route = if either("protocol", "tcp") {
        "TCP"
    } else if either("candidateType", "srflx") {
        "UDP (hole-punched)"
    } else {
        "UDP"
    }
    .to_owned();
    let number = |name: &str| Reflect::get(&pair, &name.into()).ok()?.as_f64();
    let round_trip = number("currentRoundTripTime").map(|seconds| seconds * 1000.0);
    let lost = match (number("requestsSent"), number("responsesReceived")) {
        (Some(sent), Some(answered)) if sent > 0.0 => Some(((sent - answered) / sent).max(0.0)),
        _ => None,
    };

    Some(Reading {
        route,
        round_trip,
        lost,
        totals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_minute() {
        let mut trips = VecDeque::new();

        for second in 0..=MINUTE {
            keep(&mut trips, Some(second as f64));
        }

        assert_eq!(trips.len(), MINUTE);
        assert_eq!(trips.front(), Some(&Some(1.0)));
        assert_eq!(trips.back(), Some(&Some(MINUTE as f64)));
    }

    #[test]
    fn counts_a_second_of_traffic_and_loss() {
        let last = Totals {
            received: 100.0,
            sent: 10.0,
            packets: 50.0,
            lost: 1.0,
        };
        let now = Totals {
            received: 350.0,
            sent: 5.0,
            packets: 98.0,
            lost: 3.0,
        };
        let moved = now.since(last);

        assert_eq!(moved.received, 250.0);
        assert_eq!(moved.sent, 0.0);
        assert_eq!(moved.loss(), Some(0.04));
        assert_eq!(Totals::default().loss(), None);
    }
}
