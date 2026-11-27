// A tetherd that does nothing on its own. It turns everything the page sends
// into an event, and the test answers each one through the methods the event
// carries. The page's WebRTC connection ends in a blank tab (peer.ts), whose
// WebSocket `routeWebSocket` hands to this file, so the bytes are real
// DataChannel messages with nothing listening on a port.

import type { BrowserContext, Page, WebSocketRoute } from "@playwright/test";
import {
	Gather,
	decodeConnect,
	decodeMedia,
	decodeToServer,
	encodeAdmitted,
	encodeHello,
	encodeOpened,
	encodeResult,
	encodeToClient,
	parts,
} from "./protocol";
import type {
	Connect,
	DeviceId,
	Gate,
	Hello,
	Media,
	Request,
	ToClient,
	Topic,
	UsbPhone,
	Value,
} from "./protocol";
import { peer } from "./peer";

/**
 * The origin the page is served from: dev's `playwright` process (nix/dev.nix),
 * or miniserve in the nix check (crates/web/package.nix). The page's
 * signalling and the peer never reach it, because routes answer them.
 */
const HOST = `localhost:${process.env.PLAYWRIGHT_PORT ?? 5177}`;

export const ORIGIN = `http://${HOST}`;
const PEER = `ws://${HOST}/peer`;

const WAIT = 5000;

export type Outcome = { Ok: string } | { Err: string };

export const ok = (line: string): Outcome => ({ Ok: line });
export const err = (line: string): Outcome => ({ Err: line });

export class Channel {
	private readonly came: Uint8Array[] = [];
	private readonly waiting: ((message: Uint8Array) => void)[] = [];
	private readonly gather = new Gather();
	private shut!: () => void;
	closed = false;
	readonly gone = new Promise<void>((done) => {
		this.shut = done;
	});

	constructor(
		private readonly tetherd: Tetherd,
		readonly id: number,
		readonly label: string,
	) {}

	write(bytes: Uint8Array) {
		this.tetherd.frame(this.id, bytes);
	}

	/** Sends a whole message cut into parts, as on a lane. */
	send(message: Uint8Array) {
		for (const part of parts(message)) {
			this.write(part);
		}
	}

	/** Ends the channel the way tetherd does, with a text message. */
	end() {
		this.tetherd.tell({ end: this.id });
	}

	close() {
		this.tetherd.tell({ close: this.id });
	}

	read(): Promise<Uint8Array> {
		return this.take(`a message on ${JSON.stringify(this.label)}`);
	}

	/** The next whole message the page sends, put back together from its parts. */
	async receive(): Promise<Uint8Array> {
		for (;;) {
			const whole = this.gather.add(await this.read());

			if (whole) {
				return whole;
			}
		}
	}

	/** @internal The next message, waiting as long as it takes when `what` is undefined. */
	take(what?: string): Promise<Uint8Array> {
		const message = this.came.shift();

		if (message) {
			return Promise.resolve(message);
		}

		return wait<Uint8Array>((took) => queue(this.waiting, took), what);
	}

	/** @internal */
	closed_() {
		this.closed = true;
		this.shut();
	}

	/** @internal */
	came_(bytes: Uint8Array) {
		const took = this.waiting.shift();

		if (took) {
			took(bytes);
		} else {
			this.came.push(bytes);
		}
	}
}

type Base<K extends string, T> = { kind: K } & T;

export type TetherdEvent =
	| Base<"Hello", { reply(hello?: Partial<Hello>): Promise<void> }>
	| Base<
			"Connect",
			{
				connect: Connect;
				admit(you?: string): Promise<void>;
				gate(gate: Gate): Promise<void>;
				fail(): Promise<void>;
			}
	  >
	| Base<"Subscribe", { topic: Topic }>
	| Base<"Unsubscribe", { topic: Topic }>
	| Base<"Query", { topic: Topic; send(value: Value): void }>
	| Base<
			"Call",
			{
				id: number;
				request: Request;
				reply(outcome: Outcome): void;
			}
	  >
	| Base<"Event", { what: string; failed: boolean; message: string; about: DeviceId | undefined }>
	| Base<"Offer", { id: number; sdp: string; answer(): Promise<void> }>
	| Base<"Usb", { phones: UsbPhone[] }>
	| Base<
			"Lane",
			{
				job: string;
				media: Media;
				channel: Channel;
				reply(outcome: Outcome): void;
			}
	  >
	| Base<
			"Adb",
			{
				/** The device's id as one line, such as `android:R58M`. */
				device: string;
				service: string;
				channel: Channel;
				open(): void;
				refuse(message: string): void;
			}
	  >;

export type Kind = TetherdEvent["kind"];
export type EventOf<K extends Kind> = Extract<TetherdEvent, { kind: K }>;

export type Match<K extends Kind> = DeepPartial<EventOf<K>> | ((event: EventOf<K>) => boolean);

type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;

export class Tetherd {
	readonly events: TetherdEvent[] = [];
	private readonly pending: TetherdEvent[] = [];
	private readonly waiters: { take(event: TetherdEvent): boolean }[] = [];
	private readonly ignored: ((event: TetherdEvent) => boolean)[] = [];
	private readonly standing: ((event: TetherdEvent) => boolean)[] = [];
	private readonly channels = new Map<number, Channel>();
	private readonly answers: ((sdp: string) => void)[] = [];
	private socket!: WebSocketRoute;
	private controlChannel: Channel | undefined;

	private constructor(readonly peerPage: Page) {}

	static async start(context: BrowserContext): Promise<Tetherd> {
		const peerPage = await context.newPage();
		const tetherd = new Tetherd(peerPage);
		let connected!: () => void;
		const peerConnected = new Promise<void>((done) => {
			connected = done;
		});

		await context.routeWebSocket(PEER, (socket) => {
			tetherd.socket = socket;
			socket.onMessage((message) => tetherd.heard(message));
			connected();
		});

		await context.route(`${ORIGIN}/rtc/hello`, (route) => {
			tetherd.add({
				kind: "Hello",
				reply: (hello) =>
					route.fulfill({
						body: Buffer.from(encodeHello({ nonce: [0], stun: undefined, ...hello })),
					}),
			});
		});
		await context.route(`${ORIGIN}/rtc/connect`, (route) => {
			const connect = decodeConnect(route.request().postDataBuffer() ?? new Uint8Array());
			const admitted = (body: Uint8Array) => route.fulfill({ body: Buffer.from(body) });

			tetherd.add({
				kind: "Connect",
				connect,
				admit: async (you = "SHA256:test") => {
					const answer = await tetherd.answer(connect.offer, true);

					await admitted(encodeAdmitted({ Answer: { answer, you } }));
				},
				gate: (gate) => admitted(encodeAdmitted({ Gate: gate })),
				fail: () => route.fulfill({ status: 503 }),
			});
		});

		// routeWebSocket does not catch a socket from about:blank, so the peer
		// gets an empty page on the page's origin.
		await context.route(`${ORIGIN}/peer`, (route) =>
			route.fulfill({ contentType: "text/html", body: "<!doctype html>" }),
		);
		await peerPage.goto(`${ORIGIN}/peer`);
		await peerPage.evaluate(peer, PEER);
		await peerConnected;

		return tetherd;
	}

	/** Events of other kinds stay queued for their own `next`. */
	next<K extends Kind>(kind: K, match?: Match<K>): Promise<EventOf<K>> {
		const at = this.pending.findIndex((event) => fits(event, kind, match));

		if (at >= 0) {
			return Promise.resolve(this.pending.splice(at, 1)[0] as EventOf<K>);
		}

		return wait<EventOf<K>>(
			(took) =>
				queue(this.waiters, {
					take: (event) => {
						if (!fits(event, kind, match)) {
							return false;
						}

						took(event);

						return true;
					},
				}),
			`${kind}${match && typeof match !== "function" ? ` ${JSON.stringify(match)}` : ""}`,
			() => this.pending,
		);
	}

	/**
	 * Handles queued and future events until the returned function is called.
	 * For requests the page repeats in an order a test should not pin down.
	 */
	each<K extends Kind>(kind: K, match: Match<K> | undefined, handle: (event: EventOf<K>) => void) {
		for (let at = 0; at < this.pending.length; ) {
			const event = this.pending[at];

			if (fits(event, kind, match)) {
				this.pending.splice(at, 1);
				handle(event);
			} else {
				at++;
			}
		}

		return queue(this.standing, (event: TetherdEvent) => {
			if (!fits(event, kind, match)) {
				return false;
			}

			handle(event);

			return true;
		});
	}

	send(message: ToClient) {
		if (!this.controlChannel) {
			throw new Error("tetherd: the page has no control channel open");
		}

		this.controlChannel.send(encodeToClient(message));
	}

	stop() {
		this.tell({ stop: true });
		this.controlChannel = undefined;
	}

	/** Leaves events of this kind that match unanswered without failing the test. */
	ignore<K extends Kind>(kind: K, match?: Match<K>) {
		this.ignored.push((event) => fits(event, kind, match));
	}

	left(): TetherdEvent[] {
		return this.pending.filter((event) => !this.ignored.some((ignored) => ignored(event)));
	}

	/** @internal */
	tell(message: object) {
		this.socket.send(JSON.stringify(message));
	}

	/** @internal */
	frame(id: number, bytes: Uint8Array) {
		const frame = Buffer.alloc(4 + bytes.byteLength);

		frame.writeUInt32BE(id, 0);
		frame.set(bytes, 4);
		this.socket.send(frame);
	}

	private answer(offer: string, fresh: boolean): Promise<string> {
		const answered = wait<string>((took) => queue(this.answers, took), "answer from the peer");

		this.tell({ offer, fresh });

		return answered;
	}

	private add(event: TetherdEvent) {
		this.events.push(event);

		// A test waiting on this specific event goes before a standing answer
		// from `each`.
		const at = this.waiters.findIndex((waiter) => waiter.take(event));

		if (at >= 0) {
			this.waiters.splice(at, 1);
		} else if (!this.standing.some((handle) => handle(event))) {
			this.pending.push(event);
		}
	}

	private heard(message: string | Buffer) {
		if (typeof message !== "string") {
			const id = message.readUInt32BE(0);

			this.channels.get(id)?.came_(new Uint8Array(message.subarray(4)));

			return;
		}

		const told = JSON.parse(message);

		if ("answer" in told) {
			this.answers.shift()?.(told.answer);
		} else if ("failed" in told) {
			console.error(`peer failed: ${told.failed}`);
		} else if ("opened" in told) {
			this.opened(new Channel(this, told.opened, told.label));
		} else if ("closed" in told) {
			const channel = this.channels.get(told.closed);

			if (channel) {
				channel.closed_();
				this.channels.delete(told.closed);

				if (channel === this.controlChannel) {
					this.controlChannel = undefined;
				}
			}
		}
	}

	private opened(channel: Channel) {
		this.channels.set(channel.id, channel);

		if (channel.label === "control") {
			this.controlChannel = channel;
			void this.pump(channel);
		} else if (channel.label === "media") {
			void this.first(channel).then((first) => {
				const media = decodeMedia(first);

				this.add({
					kind: "Lane",
					// serde writes a job without fields as its name alone.
					job: typeof media === "string" ? media : Object.keys(media)[0],
					media,
					channel,
					reply: (outcome) => {
						channel.send(encodeResult(outcome));
						channel.end();
					},
				});
			});
		} else if (channel.label.startsWith("adb\n")) {
			const [, device, service] = channel.label.split("\n");

			this.add({
				kind: "Adb",
				device,
				service,
				channel,
				open: () => channel.write(encodeOpened({ Ok: null })),
				refuse: (message) => {
					channel.write(encodeOpened({ Err: message }));
					channel.end();
				},
			});
		}
	}

	private async first(channel: Channel): Promise<Uint8Array> {
		const gather = new Gather();

		for (;;) {
			const whole = gather.add(await channel.take());

			if (whole) {
				return whole;
			}
		}
	}

	private async pump(channel: Channel) {
		const gather = new Gather();

		while (!channel.closed) {
			const whole = gather.add(await channel.take());

			if (whole) {
				this.control_(channel, decodeToServer(whole));
			}
		}
	}

	private control_(channel: Channel, message: ReturnType<typeof decodeToServer>) {
		const reply = (id: number, result: Outcome) =>
			channel.send(encodeToClient({ Reply: { id, result } }));

		if ("Call" in message) {
			const { id, request } = message.Call;

			this.add({ kind: "Call", id, request, reply: (outcome) => reply(id, outcome) });
		} else if ("Subscribe" in message) {
			this.add({ kind: "Subscribe", topic: message.Subscribe });
		} else if ("Unsubscribe" in message) {
			this.add({ kind: "Unsubscribe", topic: message.Unsubscribe });
		} else if ("Query" in message) {
			this.add({
				kind: "Query",
				topic: message.Query,
				send: (value) => channel.send(encodeToClient({ Value: value })),
			});
		} else if ("Event" in message) {
			this.add({ kind: "Event", ...message.Event });
		} else if ("Offer" in message) {
			const { id, sdp } = message.Offer;

			this.add({
				kind: "Offer",
				id,
				sdp,
				answer: async () => reply(id, ok(await this.answer(sdp, false))),
			});
		} else if ("Usb" in message) {
			this.add({ kind: "Usb", phones: message.Usb });
		}
	}
}

function fits<K extends Kind>(event: TetherdEvent, kind: K, match?: Match<K>): event is EventOf<K> {
	if (event.kind !== kind) {
		return false;
	}

	if (match === undefined) {
		return true;
	}

	return typeof match === "function" ? match(event as EventOf<K>) : contains(event, match);
}

export function contains(value: unknown, pattern: unknown): boolean {
	if (pattern === null || typeof pattern !== "object") {
		return Object.is(value, pattern);
	}

	if (value === null || typeof value !== "object") {
		return false;
	}

	return Object.entries(pattern).every(([key, part]) =>
		contains((value as Record<string, unknown>)[key], part),
	);
}

function queue<T>(list: T[], item: T): () => void {
	list.push(item);

	return () => {
		const at = list.indexOf(item);

		if (at >= 0) {
			list.splice(at, 1);
		}
	};
}

// The waits of the running test. A worker runs one test at a time.
const timers = new Set<ReturnType<typeof setTimeout>>();

/**
 * Drops the running test's waits, at its end. A wait it never needed, such as
 * for the answer to a renegotiation `noScreens` started without awaiting, would
 * otherwise fail in the next test.
 */
export function endWaits() {
	for (const timer of timers) {
		clearTimeout(timer);
	}

	timers.clear();
}

/**
 * `register` returns the way to stop waiting, which the timeout uses so a
 * late value is not lost to a waiter that gave up. Without `what` it waits as
 * long as it takes.
 */
function wait<T>(
	register: (took: (value: T) => void) => () => void,
	what?: string,
	left?: () => TetherdEvent[],
): Promise<T> {
	return new Promise((took, fail) => {
		let timer: ReturnType<typeof setTimeout> | undefined;
		const cancel = register((value) => {
			clearTimeout(timer);
			timers.delete(timer!);
			took(value);
		});

		if (what === undefined) {
			return;
		}

		timer = setTimeout(() => {
			timers.delete(timer!);
			cancel();

			const queued = left?.().map(describe).join("\n  ") ?? "";

			fail(
				new Error(`tetherd: no ${what} within ${WAIT} ms${queued ? `\nqueued:\n  ${queued}` : ""}`),
			);
		}, WAIT);
		timers.add(timer);
	});
}

export function describe(event: TetherdEvent): string {
	const shown = Object.fromEntries(
		Object.entries(event).filter(
			([key, value]) =>
				key !== "kind" && typeof value !== "function" && !(value instanceof Channel),
		),
	);

	// SDP and file contents would drown the rest.
	const line = JSON.stringify(shown, (_, value) =>
		typeof value === "string" && value.length > 60 ? `${value.slice(0, 60)}…` : value,
	);

	return `${event.kind} ${line}`;
}
