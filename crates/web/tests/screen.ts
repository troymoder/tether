// The video is a canvas in the peer tab (peer.ts) that the test draws on, so
// the page's picture goes through a real encoder, RTP and the page's own
// <video>.

import { type Page, expect } from "@playwright/test";
import type { DeviceId } from "./protocol";
import { encodePicture, encodeScreen } from "./protocol";
import type { Channel, EventOf, Tetherd } from "./tetherd";

/** Run in the peer tab with the screen's canvas. It can use nothing outside itself. */
export type Drawing = (context: CanvasRenderingContext2D, canvas: HTMLCanvasElement) => void;

export type Touch = {
	action: "down" | "up" | "move" | "cancel";
	x: number;
	y: number;
	width: number;
	height: number;
};

export class Screen {
	constructor(
		private readonly tetherd: Tetherd,
		readonly lane: EventOf<"Lane">,
		readonly mid: string,
		readonly audioMid: string,
	) {}

	/** `pitch` is in hertz and `loudness` from 0 to 1. */
	tone(pitch: number, loudness = 0.5): Promise<void> {
		return this.tetherd.peerPage.evaluate(
			([mid, pitch, loudness]) => {
				const sounds = (
					window as unknown as {
						sounds: Map<string, { oscillator: OscillatorNode; gain: GainNode }>;
					}
				).sounds;
				const sound = sounds.get(mid);

				if (!sound) {
					throw new Error(`no sound on mid ${mid}`);
				}

				sound.oscillator.frequency.value = pitch;
				sound.gain.gain.value = loudness;
			},
			[this.audioMid, pitch, loudness] as const,
		);
	}

	silence(): Promise<void> {
		return this.tone(440, 0);
	}

	get channel(): Channel {
		return this.lane.channel;
	}

	paint(colour: string): Promise<void> {
		return this.draw(
			new Function(
				"context",
				"canvas",
				`context.fillStyle = ${JSON.stringify(colour)}; context.fillRect(0, 0, canvas.width, canvas.height);`,
			) as Drawing,
		);
	}

	async draw(drawing: Drawing): Promise<void> {
		await this.tetherd.peerPage.evaluate(
			([mid, source]) => {
				const screens = (
					window as unknown as {
						screens: Map<
							string,
							{ canvas: HTMLCanvasElement; track: CanvasCaptureMediaStreamTrack }
						>;
					}
				).screens;
				const screen = screens.get(mid);

				if (!screen) {
					throw new Error(`no screen on mid ${mid}`);
				}

				const context = screen.canvas.getContext("2d");

				if (!context) {
					throw new Error("no 2d context");
				}

				new Function(`return (${source})`)()(context, screen.canvas);
				screen.track.requestFrame();
			},
			[this.mid, drawing.toString()] as const,
		);
	}

	/**
	 * The next touch the page sends, read as scrcpy's INJECT_TOUCH_EVENT
	 * (crates/android/src/scrcpy/control.rs): x and y in the video's pixels,
	 * with the size of the video the page saw.
	 */
	async touch(): Promise<Touch> {
		for (;;) {
			const message = await this.channel.receive();

			if (message[0] !== 2) {
				continue;
			}

			const view = new DataView(message.buffer, message.byteOffset);

			return {
				action: (["down", "up", "move", "cancel"] as const)[message[1]],
				x: view.getInt32(10),
				y: view.getInt32(14),
				width: view.getUint16(18),
				height: view.getUint16(20),
			};
		}
	}

	clipboard(text: string) {
		this.channel.send(encodeScreen({ Clipboard: text }));
	}

	fail(reason: string) {
		this.channel.send(encodeScreen({ Failed: reason }));
		this.channel.end();
	}
}

export async function screen(tetherd: Tetherd): Promise<Screen> {
	await (await tetherd.next("Offer")).answer();

	const lane = await tetherd.next("Lane", { job: "Screen" });
	const media = lane.media as { Screen: { video: string; audio: string } };

	// Chromium sends a canvas as VP8, which the page plays however it is named.
	lane.channel.send(encodeScreen({ Codec: "vp8" }));

	return new Screen(tetherd, lane, media.Screen.video, media.Screen.audio);
}

/**
 * True if the page hears about this pitch loudest. The analyser tells pitches
 * apart only to its bins, about 12 Hz wide at 48 kHz, so that is the leeway.
 */
export async function hears(page: Page, pitch: number): Promise<boolean> {
	return Math.abs((await heard(page)) - pitch) <= 12;
}

/**
 * The pitch in hertz, or 0 when it is silent. Read from the audio track the
 * page's <video> plays, through an analyser, so it is what reached the page
 * after Opus and RTP.
 */
export function heard(page: Page): Promise<number> {
	return page.evaluate(async (selector) => {
		const video = document.querySelector<HTMLVideoElement>(selector);
		const track = (video?.srcObject as MediaStream | null)?.getAudioTracks()[0];

		if (!track) {
			return 0;
		}

		const context = new AudioContext();
		const analyser = context.createAnalyser();

		await context.resume();
		analyser.fftSize = 4096;
		context.createMediaStreamSource(new MediaStream([track])).connect(analyser);
		// Long enough for the analyser to fill with the current sound.
		await new Promise((done) => setTimeout(done, 200));

		const levels = new Float32Array(analyser.frequencyBinCount);

		analyser.getFloatFrequencyData(levels);
		await context.close();

		let loudest = 0;

		for (let at = 1; at < levels.length; at++) {
			if (levels[at] > levels[loudest]) {
				loudest = at;
			}
		}

		// Below -70 dB counts as silence.
		return levels[loudest] > -70 ? (loudest * context.sampleRate) / analyser.fftSize : 0;
	}, VIDEO);
}

export const SURFACE = "#phone canvas[tabindex='0']";

const VIDEO = "#phone video";

/** A colour as red, green and blue from 0 to 255. */
export type Colour = [number, number, number];

/**
 * Null before the screen shows anything. With `on: "picture"` it reads the
 * canvas an emulator's pictures are drawn on while it boots.
 */
export function seen(page: Page, on: "video" | "picture" = "video"): Promise<Colour | null> {
	return page.evaluate(
		(selector) => {
			const shown = document.querySelector<HTMLVideoElement | HTMLCanvasElement>(selector);
			const [width, height] =
				shown instanceof HTMLVideoElement
					? [shown.videoWidth, shown.videoHeight]
					: [shown?.width ?? 0, shown?.height ?? 0];
			const canvas = document.createElement("canvas");
			const context = canvas.getContext("2d");

			if (!shown || width === 0 || !context) {
				return null;
			}

			canvas.width = width;
			canvas.height = height;
			context.drawImage(shown, 0, 0);

			const [r, g, b] = context.getImageData(width / 2, height / 2, 1, 1).data;

			return [r, g, b] as Colour;
		},
		on === "video" ? VIDEO : SURFACE,
	);
}

/**
 * Video compression and the browser's colour conversion move a colour a
 * little, so each channel may be off by up to 48.
 */
export async function shows(page: Page, wanted: Colour, on: "video" | "picture" = "video") {
	await expect(async () => {
		const colour = await seen(page, on);

		expect(
			colour?.every((value, at) => Math.abs(value - wanted[at]) <= 48) ?? false,
			`the ${on} shows ${JSON.stringify(colour)}, wanted about ${JSON.stringify(wanted)}`,
		).toBe(true);
	}).toPass({ timeout: 5_000 });
}

export async function watch(
	tetherd: Tetherd,
	device: DeviceId,
): Promise<{ lane: EventOf<"Lane">; picture(colour: [number, number, number]): void }> {
	const lane = await tetherd.next("Lane", { media: { Watch: { device } } });

	return {
		lane,
		picture: ([red, green, blue]) => {
			const width = 180;
			const height = 320;
			const rgba = new Uint8Array(width * height * 4);

			for (let at = 0; at < rgba.length; at += 4) {
				rgba.set([red, green, blue, 255], at);
			}

			lane.channel.send(encodePicture(width, height, rgba));
		},
	};
}
