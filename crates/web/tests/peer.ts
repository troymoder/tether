// The fixture runs this function in a blank tab with `evaluate`, so only its
// own source reaches the tab. It cannot use imports or anything outside
// itself. It is a pipe between the RTCPeerConnection and a WebSocket that
// `routeWebSocket` hands to the test (tetherd.ts).
//
// Text frames are JSON: the test sends `{ offer, fresh }`, `{ close: id }`,
// `{ end: id }` and `{ stop: true }`, and the peer sends `{ answer }`,
// `{ failed }` when it cannot answer, `{ opened: id, label }` and
// `{ closed: id }`. Binary frames are a channel's messages in either
// direction: a 4-byte channel id, then the message.
//
// Each video track the page offers to receive is sent a canvas, which the
// browser encodes as it would a camera. The canvases are in `window.screens`
// by mid, for the test to draw on with `evaluate` (screen.ts).
export function peer(url: string) {
	const socket = new WebSocket(url);
	const channels = new Map<number, RTCDataChannel>();
	const screens = new Map<
		string,
		{ canvas: HTMLCanvasElement; track: CanvasCaptureMediaStreamTrack }
	>();
	const sounds = new Map<string, { oscillator: OscillatorNode; gain: GainNode }>();
	let audio: AudioContext | undefined;
	let connection: RTCPeerConnection | undefined;
	let next = 0;

	socket.binaryType = "arraybuffer";
	Object.assign(window, { screens, sounds });

	const tell = (message: object) => socket.send(JSON.stringify(message));

	// The page's decoder starts at a key frame, which the encoder only makes
	// from a frame it is given, so the canvases are sent again ten times a
	// second when nothing is drawn on them. The ticks come from a worker,
	// because Chrome throttles a hidden tab's own timers to one a second and
	// leaves a worker's alone.
	const ticker = new Worker(
		URL.createObjectURL(
			new Blob(["setInterval(() => postMessage(0), 100)"], { type: "text/javascript" }),
		),
	);

	ticker.onmessage = () => {
		for (const { track } of screens.values()) {
			track.requestFrame();
		}
	};

	const film = async (pc: RTCPeerConnection) => {
		for (const transceiver of pc.getTransceivers()) {
			const mid = transceiver.mid;

			// The page removed this track, as when its screen closed.
			if (mid && transceiver.currentDirection === "stopped") {
				screens.get(mid)?.track.stop();
				screens.delete(mid);
				sounds.get(mid)?.oscillator.stop();
				sounds.delete(mid);
				continue;
			}

			if (!mid || screens.has(mid) || sounds.has(mid)) {
				continue;
			}

			if (transceiver.receiver.track.kind === "audio") {
				audio ??= new AudioContext();

				const oscillator = audio.createOscillator();
				const gain = audio.createGain();
				const out = audio.createMediaStreamDestination();

				gain.gain.value = 0;
				oscillator.connect(gain).connect(out);
				oscillator.start();
				sounds.set(mid, { oscillator, gain });
				transceiver.direction = "sendonly";
				await transceiver.sender.replaceTrack(out.stream.getAudioTracks()[0]);
				continue;
			}

			const canvas = document.createElement("canvas");
			const track = canvas.captureStream(0).getVideoTracks()[0] as CanvasCaptureMediaStreamTrack;
			const context = canvas.getContext("2d");

			canvas.width = 180;
			canvas.height = 320;

			if (context) {
				context.fillStyle = "black";
				context.fillRect(0, 0, canvas.width, canvas.height);
			}

			screens.set(mid, { canvas, track });
			transceiver.direction = "sendonly";
			await transceiver.sender.replaceTrack(track);
		}
	};

	const fresh = () => {
		connection?.close();
		channels.clear();
		screens.clear();
		sounds.clear();
		connection = new RTCPeerConnection();
		connection.ondatachannel = ({ channel }) => {
			const id = next++;

			channels.set(id, channel);
			channel.binaryType = "arraybuffer";
			tell({ opened: id, label: channel.label });

			channel.onmessage = ({ data }) => {
				const body = new Uint8Array(data as ArrayBuffer);
				const frame = new Uint8Array(4 + body.byteLength);

				new DataView(frame.buffer).setUint32(0, id);
				frame.set(body, 4);
				socket.send(frame);
			};

			channel.onclose = () => {
				channels.delete(id);
				tell({ closed: id });
			};
		};

		return connection;
	};

	socket.onmessage = async ({ data }) => {
		if (typeof data !== "string") {
			const id = new DataView(data as ArrayBuffer).getUint32(0);

			channels.get(id)?.send(new Uint8Array(data as ArrayBuffer, 4));

			return;
		}

		const message = JSON.parse(data);

		if ("offer" in message) {
			const pc = message.fresh || !connection ? fresh() : connection;

			try {
				await pc.setRemoteDescription({ type: "offer", sdp: message.offer });
				await film(pc);
				await pc.setLocalDescription(await pc.createAnswer());
			} catch (error) {
				tell({ failed: String(error) });
				return;
			}

			// The page does not trickle candidates, so the answer carries all of
			// this side's. This waits on the event because a hidden tab's timers
			// are throttled to one a second, which a polling loop would wait on.
			if (pc.iceGatheringState !== "complete") {
				await new Promise<void>((done) =>
					pc.addEventListener("icegatheringstatechange", () => {
						if (pc.iceGatheringState === "complete") {
							done();
						}
					}),
				);
			}

			tell({ answer: pc.localDescription?.sdp });
		} else if ("close" in message) {
			channels.get(message.close)?.close();
		} else if ("end" in message) {
			// tetherd ends a lane or an adb channel with a text message.
			channels.get(message.end)?.send("");
		} else if ("stop" in message) {
			connection?.close();
			connection = undefined;
			channels.clear();
			screens.clear();
			sounds.clear();
		}
	};

	return new Promise<void>((open) => {
		socket.onopen = () => open();
	});
}
