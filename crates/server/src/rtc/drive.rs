use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use str0m::channel::ChannelId;
use str0m::format::Codec;
use str0m::media::{Frequency, MediaTime, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Event, IceConnectionState, Input, Output, Rtc};
use tokio::sync::mpsc;

use super::{Asked, Credit, Peer, Said, Sample, Sockets, Track};

struct Joined {
    peer: Peer,
    rtc: Box<Rtc>,
    said: mpsc::UnboundedSender<Said>,
    wake: Instant,
    waiting: HashMap<ChannelId, VecDeque<Waiting>>,
    lost: u32,
    told: Instant,
    /// The time the connection joined, which video's RTP time counts from.
    since: Instant,
    /// The time ICE last said it was disconnected, while it has not recovered.
    quiet: Option<Instant>,
}

/// A message waiting to go, the window it holds, and when it was asked for.
type Waiting = (Vec<u8>, Option<Credit>, Instant);

/// A message held this long before the connection takes it is logged: the
/// data channel had no room, as when the page reads it too slowly.
const HELD: Duration = Duration::from_millis(250);

/// The text message that ends a channel, whose page then closes it.
const END: &[u8] = b"end";

/// The time a disconnected connection is given to recover, on its path or
/// another, before it is dropped. A browser fails its own side after the
/// same 30 s of consent going unanswered (RFC 7675).
const RECOVERING: Duration = Duration::from_secs(30);

type TcpConnections = HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>;

impl Joined {
    fn flush(&mut self) {
        let mut failed = Vec::new();

        for (channel, queue) in &mut self.waiting {
            while let Some((data, _, asked)) = queue.front() {
                let Some(mut open) = self.rtc.channel(*channel) else {
                    queue.clear();
                    break;
                };

                // An empty message is the end of the channel (`Hub::close`),
                // which goes as text. str0m sends nothing for an empty one.
                let written = if data.is_empty() {
                    open.write(false, END)
                } else {
                    open.write(true, data)
                };

                match written {
                    Ok(true) => {
                        if asked.elapsed() >= HELD {
                            tracing::info!(
                                "webrtc {}: a message waited {} ms for room on its channel, {} more behind it",
                                self.peer,
                                asked.elapsed().as_millis(),
                                queue.len() - 1
                            );
                        }

                        if let Some((_, Some(Credit(window, bytes)), _)) = queue.pop_front() {
                            window.add_permits(bytes as usize);
                        }
                    }
                    Ok(false) => break,
                    Err(error) => {
                        tracing::debug!("webrtc {}: write failed: {error}", self.peer);
                        queue.clear();
                        failed.push(*channel);
                        break;
                    }
                }
            }
        }

        self.waiting.retain(|_, queue| !queue.is_empty());

        // Its end was lost with the rest, so the page learns of it this way.
        for channel in failed {
            self.rtc.direct_api().close_data_channel(channel);
        }
    }

    fn drive(&mut self, sockets: &Sockets, tcp: &TcpConnections) {
        loop {
            match self.rtc.poll_output() {
                Ok(Output::Timeout(at)) => {
                    self.wake = at;

                    if self
                        .quiet
                        .is_some_and(|quiet| quiet.elapsed() >= RECOVERING)
                    {
                        tracing::debug!("webrtc {}: did not recover, dropped", self.peer);
                        self.rtc.disconnect();
                    }

                    // Told at most once a second, since a full socket loses
                    // many at once.
                    if self.lost > 0 && self.told.elapsed() >= Duration::from_secs(1) {
                        tracing::info!(
                            "webrtc {}: {} packets lost to a full socket",
                            self.peer,
                            self.lost
                        );
                        self.lost = 0;
                        self.told = Instant::now();
                    }

                    return;
                }
                Ok(Output::Transmit(sent)) => match sent.proto {
                    Protocol::Udp => {
                        let socket = sockets.lock().unwrap().get(&sent.source.ip()).cloned();

                        // A packet that does not fit now is lost, as UDP
                        // packets can be. The `Rtc` resends what matters, and
                        // its channel waits for that resend.
                        if let Some(socket) = socket
                            && let Err(error) = socket.try_send_to(&sent.contents, sent.destination)
                        {
                            // Others are probes of addresses that cannot be
                            // reached from this one, which ICE tries anyway.
                            if error.kind() == std::io::ErrorKind::WouldBlock {
                                self.lost += 1;
                            } else {
                                tracing::trace!(
                                    "webrtc {}: {} to {}: {error}",
                                    self.peer,
                                    sent.source,
                                    sent.destination
                                );
                            }
                        }
                    }
                    _ => {
                        if let Some(connection) = tcp.get(&sent.destination) {
                            let _ = connection.send(sent.contents.to_vec());
                        }
                    }
                },
                Ok(Output::Event(event)) => self.passed(event),
                Err(error) => {
                    tracing::warn!("webrtc failed: {error}");
                    self.rtc.disconnect();
                }
            }
        }
    }

    fn passed(&mut self, event: Event) {
        match event {
            Event::IceConnectionStateChange(state) => {
                tracing::debug!("webrtc {}: ice {state:?}", self.peer);

                // A disconnected connection can still recover, as the
                // browser goes on checking on its path or finds another.
                match state {
                    IceConnectionState::Disconnected => {
                        self.quiet.get_or_insert_with(Instant::now);
                    }
                    IceConnectionState::Connected | IceConnectionState::Completed => {
                        self.quiet = None;
                    }
                    _ => {}
                }
            }
            Event::Connected => tracing::debug!("webrtc {}: connected", self.peer),
            Event::ChannelOpen(channel, label) => {
                tracing::debug!("webrtc {}: channel {label:?}", self.peer);
                let _ = self.said.send(Said::Opened(channel, label));
            }
            Event::ChannelData(data) => {
                let _ = self.said.send(Said::Data(data.id, data.data));
            }
            Event::ChannelClose(channel) => {
                self.waiting.remove(&channel);
                let _ = self.said.send(Said::Closed(channel));
            }
            Event::KeyframeRequest(request) => {
                let _ = self.said.send(Said::KeyFrame(request.mid));
            }
            _ => {}
        }
    }

    /// Writes a piece of a track, in the payload type the browser agreed to
    /// for it: Opus, or H.264 with fragmenting allowed, of the stream's
    /// profile where the browser offered it. Dropped before the connection
    /// is up, as str0m drops it then too.
    fn media(&mut self, mid: Mid, sample: &Sample) {
        let Some(writer) = self.rtc.writer(mid) else {
            return;
        };
        let pt = match sample.track {
            Track::Audio => writer
                .payload_params()
                .find(|params| params.spec().codec == Codec::Opus)
                .map(|params| params.pt()),
            Track::Video { profile } => {
                let h264 = || {
                    writer.payload_params().filter(|params| {
                        let spec = params.spec();

                        spec.codec == Codec::H264 && spec.format.packetization_mode == Some(1)
                    })
                };

                h264()
                    .find(|params| {
                        params
                            .spec()
                            .format
                            .profile_level_id
                            .is_some_and(|id| (id >> 16) as u8 == profile)
                    })
                    .or_else(|| h264().next())
                    .map(|params| params.pt())
            }
        };
        let Some(pt) = pt else {
            return;
        };
        let rate = match sample.track {
            Track::Audio => Frequency::FORTY_EIGHT_KHZ,
            Track::Video { .. } => Frequency::NINETY_KHZ,
        };
        let now = Instant::now();
        // Video is timed by when it is sent. The device holds frames back now
        // and then, and with the device's clock the browser would take that
        // gap for network jitter and buffer frames to smooth it over. Sound
        // keeps the device's even steps, which its decoder plays by.
        let micros = match sample.track {
            Track::Video { .. } => (now - self.since).as_micros() as u64,
            Track::Audio => sample.time,
        };
        let rtp_time = MediaTime::new(micros * u64::from(rate.get()) / 1_000_000, rate);
        let mut writer = writer;

        // Asks the browser to show each frame as soon as it is decoded,
        // where it agreed to the playout delay extension.
        if matches!(sample.track, Track::Video { .. }) {
            writer = writer.playout_delay(MediaTime::ZERO, MediaTime::ZERO);
        }

        if let Err(error) = writer.write(pt, now, rtp_time, sample.data.clone()) {
            tracing::debug!("webrtc {}: {error}", self.peer);
        }
    }
}

enum Woken {
    Asked(Asked),
    Time,
    Ended,
}

pub(super) async fn run(mut asked: mpsc::UnboundedReceiver<Asked>, sockets: Sockets) {
    let mut joined: Vec<Joined> = Vec::new();
    let mut tcp = TcpConnections::new();

    loop {
        let now = Instant::now();

        for one in &mut joined {
            if one.wake <= now {
                let _ = one.rtc.handle_input(Input::Timeout(now));
            }

            one.flush();
            one.drive(&sockets, &tcp);
        }

        joined.retain(|one| one.rtc.is_alive() && !one.said.is_closed());

        let wake = joined
            .iter()
            .map(|one| one.wake)
            .min()
            .unwrap_or(now + Duration::from_secs(1));
        let woken = tokio::select! {
            got = asked.recv() => got.map_or(Woken::Ended, Woken::Asked),
            () = tokio::time::sleep_until(wake.into()) => Woken::Time,
        };

        match woken {
            Woken::Asked(asked) => take(asked, &mut joined, &mut tcp),
            Woken::Time => {}
            Woken::Ended => return,
        }
    }
}

fn take(asked: Asked, joined: &mut Vec<Joined>, tcp: &mut TcpConnections) {
    match asked {
        Asked::Join { peer, rtc, said } => joined.push(Joined {
            peer,
            rtc,
            said,
            wake: Instant::now(),
            waiting: HashMap::new(),
            lost: 0,
            told: Instant::now(),
            since: Instant::now(),
            quiet: None,
        }),
        Asked::Receive {
            proto,
            from,
            to,
            data,
        } => {
            let Ok(contents) = data.as_slice().try_into() else {
                return;
            };

            let input = Input::Receive(
                Instant::now(),
                Receive {
                    proto,
                    source: from,
                    destination: to,
                    contents,
                },
            );

            if let Some(one) = joined.iter_mut().find(|one| one.rtc.accepts(&input)) {
                let _ = one.rtc.handle_input(input);
            }
        }
        Asked::TcpOpened(from, out) => {
            tcp.insert(from, out);
        }
        Asked::TcpClosed(from) => {
            tcp.remove(&from);
        }
        Asked::Write {
            peer,
            channel,
            data,
            credit,
        } => {
            if let Some(one) = joined.iter_mut().find(|one| one.peer == peer) {
                one.waiting
                    .entry(channel)
                    .or_default()
                    .push_back((data, credit, Instant::now()));
            }
        }
        Asked::Close(peer, channel) => {
            // The empty message stands for `END`, which `flush` sends after
            // the messages before it.
            if let Some(one) = joined.iter_mut().find(|one| one.peer == peer) {
                one.waiting.entry(channel).or_default().push_back((
                    Vec::new(),
                    None,
                    Instant::now(),
                ));
            }
        }
        Asked::Leave(peer) => {
            if let Some(one) = joined.iter_mut().find(|one| one.peer == peer) {
                one.rtc.disconnect();
            }
        }
        Asked::Offer {
            peer,
            offer,
            answer,
        } => {
            let answered = match joined.iter_mut().find(|one| one.peer == peer) {
                Some(one) => one
                    .rtc
                    .sdp_api()
                    .accept_offer(offer)
                    .map(|answer| answer.to_sdp_string())
                    .map_err(|error| error.to_string()),
                None => Err("the connection ended".to_owned()),
            };

            let _ = answer.send(answered);
        }
        Asked::Media { peer, mid, sample } => {
            if let Some(one) = joined.iter_mut().find(|one| one.peer == peer) {
                one.media(mid, &sample);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::net::{Ipv4Addr, SocketAddr};
    use std::time::{Duration, Instant};

    use str0m::channel::{ChannelConfig, ChannelId};
    use str0m::net::{Protocol, Receive};
    use str0m::{Candidate, Event, Input, Output, Rtc};
    use tokio::sync::mpsc;

    use super::{Joined, take};
    use crate::rtc::{Asked, Said};

    type Packet = (SocketAddr, SocketAddr, Vec<u8>);

    fn poll(rtc: &mut Rtc, out: &mut VecDeque<Packet>) -> Vec<Event> {
        let mut events = Vec::new();

        loop {
            match rtc.poll_output().unwrap() {
                Output::Timeout(_) => return events,
                Output::Transmit(sent) => {
                    out.push_back((sent.source, sent.destination, sent.contents.to_vec()));
                }
                Output::Event(event) => events.push(event),
            }
        }
    }

    fn deliver(rtc: &mut Rtc, now: Instant, packets: &mut VecDeque<Packet>) {
        for (from, to, data) in packets.drain(..) {
            let receive = Receive {
                proto: Protocol::Udp,
                source: from,
                destination: to,
                contents: data.as_slice().try_into().unwrap(),
            };

            rtc.handle_input(Input::Receive(now, receive)).unwrap();
        }
    }

    /// A page that opens many short adb channels in a row, as the Database view
    /// runs its shells, against tetherd's driving task in memory. The relay
    /// answers each after a few steps with its first message and ends it, and
    /// the page closes half of them itself as soon as that arrives, as libadb
    /// does on a shell's exit. Every channel the page opened must have its
    /// first message before it closes.
    ///
    /// This fails if tetherd resets channels too (`Hub::close`), with str0m
    /// logging "Failed to write DCEP ack".
    #[test]
    fn first_message_comes_before_close() {
        let mut now = Instant::now();
        let mut browser = Rtc::builder().build(now);
        let mut server = Rtc::builder().build(now);
        let browser_at = SocketAddr::from((Ipv4Addr::new(1, 1, 1, 1), 1000));
        let server_at = SocketAddr::from((Ipv4Addr::new(2, 2, 2, 2), 2000));

        browser.add_local_candidate(Candidate::host(browser_at, "udp").unwrap());
        server.add_local_candidate(Candidate::host(server_at, "udp").unwrap());

        let mut change = browser.sdp_api();

        change.add_channel("control".to_owned());

        let (offer, pending) = change.apply().unwrap();
        let answer = server.sdp_api().accept_offer(offer).unwrap();

        browser.sdp_api().accept_answer(pending, answer).unwrap();

        let (said, mut hearing) = mpsc::unbounded_channel();
        let mut joined = vec![Joined {
            peer: 0,
            rtc: Box::new(server),
            said,
            wake: now,
            waiting: HashMap::new(),
            lost: 0,
            told: now,
            since: now,
            quiet: None,
        }];
        let mut tcp = super::TcpConnections::new();
        let mut to_browser = VecDeque::new();
        let mut to_server = VecDeque::new();
        // The relays still to answer, by channel, with the step they answer at.
        let mut relays: HashMap<ChannelId, u32> = HashMap::new();
        // The page's channels by its own ids: true once the first message came.
        let mut page: HashMap<ChannelId, bool> = HashMap::new();
        let mut opened = 0;
        let mut lost = 0;
        let mut seed: u32 = 1;
        let mut random = move |below: u32| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (seed >> 16) % below
        };

        for step in 0..60_000_u32 {
            now += Duration::from_millis(1);

            let one = &mut joined[0];

            one.rtc.handle_input(Input::Timeout(now)).unwrap();
            one.flush();

            for event in poll(&mut one.rtc, &mut to_browser) {
                one.passed(event);
            }

            deliver(&mut one.rtc, now, &mut to_server);

            while let Ok(said) = hearing.try_recv() {
                match said {
                    Said::Opened(channel, _) => {
                        relays.insert(channel, step + random(4));
                    }
                    // `Channels::close` aborts the relay.
                    Said::Closed(channel) => {
                        relays.remove(&channel);
                    }
                    _ => {}
                }
            }

            let answered: Vec<ChannelId> = relays
                .iter()
                .filter(|(_, at)| **at <= step)
                .map(|(channel, _)| *channel)
                .collect();

            for channel in answered {
                relays.remove(&channel);

                for data in [b"opened".to_vec(), b"output".to_vec()] {
                    let write = Asked::Write {
                        peer: 0,
                        channel,
                        data,
                        credit: None,
                    };

                    take(write, &mut joined, &mut tcp);
                }

                take(Asked::Close(0, channel), &mut joined, &mut tcp);
            }

            browser.handle_input(Input::Timeout(now)).unwrap();

            for event in poll(&mut browser, &mut to_server) {
                match event {
                    // The end of the channel, which the page closes as
                    // link/relay.rs does.
                    Event::ChannelData(data) if !data.binary => {
                        browser.direct_api().close_data_channel(data.id);
                    }
                    Event::ChannelData(data) => {
                        if let Some(first) = page.get_mut(&data.id)
                            && !*first
                        {
                            *first = true;

                            if random(2) == 0 {
                                browser.direct_api().close_data_channel(data.id);
                            }
                        }
                    }
                    Event::ChannelClose(channel) if page.remove(&channel) == Some(false) => {
                        lost += 1;
                    }
                    _ => {}
                }
            }

            deliver(&mut browser, now, &mut to_browser);

            // A few shells at a time, each opened as soon as there is room.
            if browser.is_connected() && page.len() < 3 && random(3) == 0 {
                let config = ChannelConfig {
                    label: "adb".to_owned(),
                    ..Default::default()
                };
                let channel = browser.direct_api().create_data_channel(config);

                page.insert(channel, false);
                opened += 1;
            }
        }

        assert!(opened > 5000, "only {opened} channels opened");
        assert_eq!(
            lost, 0,
            "{lost} of {opened} closed before their first message"
        );
    }
}
