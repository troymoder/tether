//! The offer goes to tetherd over HTTP, signed with this browser's key, and
//! the answer comes back in the reply.

use std::cell::RefCell;
use std::time::Duration;

use js_sys::Uint8Array;
use leptos::prelude::*;
use protocol::{Admitted, ArchivedAdmitted, ClientId, Connect, Gate, Gather, access, encode, own};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    RequestInit, Response, RtcConfiguration, RtcDataChannel, RtcDataChannelState,
    RtcDataChannelType, RtcIceConnectionState, RtcIceGatheringState, RtcIceServer,
    RtcPeerConnection, RtcPeerConnectionState, RtcSdpType, RtcSessionDescriptionInit,
};

use super::heard::{Heard, bytes};
use super::key::{browser_name, signed};
use super::{Link, Linked};

const FIRST_DELAY: Duration = Duration::from_secs(1);

/// Without a cap, a tetherd that was down for a while would be found long
/// after it came back, while the page stays covered.
const MOST_DELAY: Duration = Duration::from_secs(10);

fn delay(failed: u32) -> Duration {
    let doublings = failed.saturating_sub(1).min(31);

    FIRST_DELAY.saturating_mul(1 << doublings).min(MOST_DELAY)
}

pub(super) struct Peer {
    pub(super) pc: RtcPeerConnection,
    pub(super) control: RtcDataChannel,
    gather: RefCell<Gather>,
    _heard: (Heard, Heard),
}

async fn post(path: &str, body: &[u8]) -> Option<Vec<u8>> {
    let init = RequestInit::new();

    init.set_method("POST");
    init.set_body(&Uint8Array::from(body));

    let response: Response = JsFuture::from(window().fetch_with_str_and_init(path, &init))
        .await
        .ok()?
        .dyn_into()
        .ok()?;

    if !response.ok() {
        return None;
    }

    let buffer = JsFuture::from(response.array_buffer().ok()?).await.ok()?;

    Some(Uint8Array::new(&buffer).to_vec())
}

impl Link {
    pub(super) fn connect(self) {
        self.stop();

        let at = self.connection(|connection| {
            let at = connection.tries.get().wrapping_add(1);

            connection.tries.set(at);

            at
        });

        leptos::task::spawn_local(async move {
            if let Some(wait) = self.connecting(at).await
                && self.current(at)
            {
                self.stop();
                // The setup steps stay shown while tetherd is asked again.
                self.relink(|linked| {
                    if let Linked::Connecting { retrying } = linked {
                        *retrying = true;
                    }
                });

                self.again(at, wait);
            }
        });
    }

    fn current(self, at: u32) -> bool {
        self.connection(|connection| connection.tries.get()) == at
    }

    fn again(self, at: u32, wait: Duration) {
        set_timeout(
            move || {
                if self.current(at) {
                    self.connect();
                }
            },
            wait,
        );
    }

    /// Returns the delay before the next attempt when it did not get a
    /// connection.
    async fn connecting(self, at: u32) -> Option<Duration> {
        let Some(hello) = post(protocol::HELLO, &[]).await else {
            return Some(self.backoff());
        };

        let Ok(protocol::Hello { nonce, stun }) =
            access::<protocol::Hello>(&hello).map(own::<protocol::Hello>)
        else {
            return Some(self.backoff());
        };

        if !self.current(at) {
            return None;
        }

        // tetherd's candidates come in its answer, and it learns this
        // browser's from the connectivity checks it receives. With a STUN
        // server this browser also learns its public address, so tetherd can
        // punch through to it there.
        let pc = match &stun {
            Some(stun) => {
                let server = RtcIceServer::new();

                server.set_urls(&JsValue::from(format!("stun:{stun}")));

                let config = RtcConfiguration::new();

                config.set_ice_servers(&js_sys::Array::of1(&server));

                RtcPeerConnection::new_with_configuration(&config)
            }
            None => RtcPeerConnection::new(),
        };

        let Ok(pc) = pc else {
            return Some(self.backoff());
        };

        let control = pc.create_data_channel(protocol::CONTROL);

        control.set_binary_type(RtcDataChannelType::Arraybuffer);

        let heard = self.listen(at, &pc, &control);

        self.connection(|connection| {
            connection.peer.replace(Some(Peer {
                pc: pc.clone(),
                control,
                gather: RefCell::default(),
                _heard: heard,
            }))
        });

        let Ok(offer) = JsFuture::from(pc.create_offer()).await else {
            return Some(self.backoff());
        };

        let Some(mut sdp) = js_sys::Reflect::get(&offer, &"sdp".into())
            .ok()
            .and_then(|sdp| sdp.as_string())
        else {
            return Some(self.backoff());
        };

        if JsFuture::from(pc.set_local_description(offer.unchecked_ref()))
            .await
            .is_err()
        {
            return Some(self.backoff());
        }

        // Without STUN there are no candidates worth waiting for. With it, the
        // offer waits up to 600 ms for the public address only, because full
        // gathering takes seconds while STUN requests on other interfaces time out.
        for _ in 0..30 {
            if stun.is_none() {
                break;
            }

            sdp = pc.local_description().map_or(sdp, |now| now.sdp());

            if sdp.contains(" typ srflx")
                || pc.ice_gathering_state() == RtcIceGatheringState::Complete
            {
                break;
            }

            crate::util::sleep(Duration::from_millis(20)).await;
        }

        // The code is taken, so a wrong one is only tried once. Only a proof
        // of it is sent, because this request may go over plain HTTP.
        let proof = self
            .connection(|connection| connection.code.take())
            .map(|code| protocol::code_proof(&code, &nonce, &sdp));

        let Some((key, signature)) =
            signed(&protocol::signed(&nonce, &sdp, proof.as_deref())).await
        else {
            self.say("key failed: no WebCrypto");

            return Some(self.backoff());
        };

        let asked = encode(&Connect {
            key,
            signature,
            name: browser_name(),
            kind: "Browser".to_owned(),
            nonce,
            offer: sdp,
            proof,
        });

        let Some(reply) = post(protocol::CONNECT, &asked).await else {
            return Some(self.backoff());
        };

        if !self.current(at) {
            return None;
        }

        let Ok(admitted) = access::<Admitted>(&reply) else {
            return Some(self.backoff());
        };

        let (answer, you) = match admitted {
            ArchivedAdmitted::Answer { answer, you } => (answer.to_string(), you.to_string()),
            // tetherd is asked again 2 s later, which is how the page learns
            // it was approved. The gate is only set when it changed, because
            // setting it restarts the setup form.
            ArchivedAdmitted::Gate(gate) => {
                let gate = own::<Gate>(gate);
                // tetherd reports a wrong code only in the answer to the
                // attempt that sent it. Later attempts without a code keep
                // that report until another code is tried.
                let still_missed = |now: &Gate| match (now, &gate) {
                    (
                        Gate::Setup {
                            primary,
                            you,
                            missed: true,
                        },
                        Gate::Setup {
                            primary: again,
                            you: same,
                            missed: false,
                        },
                    ) => primary == again && you == same,
                    _ => false,
                };

                let changed = self.linked.with_untracked(|now| match now {
                    Linked::Gate(now) => *now != gate && !still_missed(now),
                    _ => true,
                });

                if changed {
                    self.linked.set(Linked::Gate(gate));
                }

                return Some(Duration::from_secs(2));
            }
        };

        let description = RtcSessionDescriptionInit::new(RtcSdpType::Answer);

        description.set_sdp(&answer);

        if JsFuture::from(pc.set_remote_description(&description))
            .await
            .is_err()
        {
            return Some(self.backoff());
        }

        if self.current(at) {
            self.you.set(Some(ClientId(you)));
            self.relink(|linked| {
                if matches!(linked, Linked::Gate(_)) {
                    *linked = Linked::Connecting { retrying: false };
                }
            });
        }

        None
    }

    fn listen(self, at: u32, pc: &RtcPeerConnection, control: &RtcDataChannel) -> (Heard, Heard) {
        let mut heard = Heard::new(pc);
        let watched = pc.clone();

        heard.on("connectionstatechange", move |_| {
            match watched.connection_state() {
                RtcPeerConnectionState::Failed | RtcPeerConnectionState::Closed => self.lost(at),
                // Disconnected may recover, on its path or another, so it is
                // shown and given 30 s, the same as tetherd gives it
                // (`RECOVERING` in crates/server/src/rtc/drive.rs). The browser
                // also fails it once its consent checks go unanswered that long.
                RtcPeerConnectionState::Disconnected => {
                    let watched = watched.clone();

                    if self.current(at) {
                        self.interrupt(true);
                    }

                    set_timeout(
                        move || {
                            if watched.connection_state() == RtcPeerConnectionState::Disconnected {
                                self.lost(at);
                            }
                        },
                        Duration::from_secs(30),
                    );
                }
                RtcPeerConnectionState::Connected if self.current(at) => self.interrupt(false),
                _ => {}
            }
        });

        let watched = pc.clone();

        heard.on("iceconnectionstatechange", move |_| {
            let firefox = window()
                .navigator()
                .user_agent()
                .is_ok_and(|agent| agent.contains("Firefox/"));

            if watched.ice_connection_state() == RtcIceConnectionState::Failed
                && firefox
                && !self.connection(|connection| connection.told.replace(true))
            {
                self.say("webrtc failed: Firefox can't connect through a localhost port forward");
            }
        });

        let mut channel = Heard::new(control);

        // tetherd forgets a client's topics and phones when it disconnects, so
        // each new channel sends them again.
        channel.on("open", move |_| {
            self.connection(|connection| connection.failed.set(0));
            self.linked.set(Linked::Connected {
                since: js_sys::Date::now(),
                route: None,
                interrupted: false,
            });

            self.subscribed();
            self.report_usb();
        });

        channel.on("message", move |event| {
            let Some(part) = bytes(&event) else {
                return;
            };

            let whole = self.connection(|connection| {
                connection
                    .peer
                    .borrow()
                    .as_ref()
                    .and_then(|peer| peer.gather.borrow_mut().add(&part))
            });

            if let Some(whole) = whole {
                self.receive(&whole);
            }
        });

        channel.on("close", move |_| self.lost(at));

        (heard, channel)
    }

    fn lost(self, at: u32) {
        if !self.current(at) || self.connection(|connection| connection.peer.borrow().is_none()) {
            return;
        }

        self.stop();
        self.again(at, self.backoff());
    }

    fn backoff(self) -> Duration {
        let failed = self.connection(|connection| {
            connection
                .failed
                .set(connection.failed.get().saturating_add(1));
            connection.failed.get()
        });

        delay(failed)
    }

    fn stop(self) {
        // Dropping the senders fails the calls still waiting for an answer.
        let peer = self.connection(|connection| {
            connection.waiting.borrow_mut().clear();
            connection.relays.gone();
            connection.peer.take()
        });

        // Dropping the peer removes its listeners.
        if let Some(peer) = peer {
            peer.pc.close();
        }

        self.relink(|linked| {
            if matches!(linked, Linked::Connected { .. }) {
                *linked = Linked::Connecting { retrying: true };
            }
        });
    }

    fn interrupt(self, now: bool) {
        self.relink(|linked| {
            if let Linked::Connected { interrupted, .. } = linked {
                *interrupted = now;
            }
        });
    }

    pub(crate) fn claim(self, code: String) {
        self.connection(|connection| connection.code.replace(Some(code)));
        self.connect();
    }

    pub(crate) fn peer(self) -> Option<RtcPeerConnection> {
        self.connection(|connection| {
            connection
                .peer
                .borrow()
                .as_ref()
                .filter(|peer| peer.control.ready_state() == RtcDataChannelState::Open)
                .map(|peer| peer.pc.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_delay_doubles_up_to_its_cap() {
        let delays: Vec<u64> = (1..=6).map(|failed| delay(failed).as_secs()).collect();

        assert_eq!(delays, [1, 2, 4, 8, 10, 10]);
        assert_eq!(delay(u32::MAX), MOST_DELAY);
    }
}
