// tetherd gives the page one DataChannel per service (tetherd.ts), so this
// speaks the protocol of each: shell protocol v2 both ways, and sync v1 for
// push and pull. The wire formats are adbd's, as libadb implements them
// (libadb src/shell/v2.rs and src/sync.rs).

import type { DeviceId } from "./protocol";
import { line } from "./builders";
import type { Channel, EventOf, Tetherd } from "./tetherd";

export type Ran = { stdout?: string | Uint8Array; stderr?: string | Uint8Array; exit?: number };

/**
 * A program may read stdin and print through the process as it runs. Its
 * return value is printed last, then it exits.
 */
export type Answer =
	| Ran
	| string
	| ((command: string, process: Process) => Ran | string | void | Promise<Ran | string | void>);

/** The first that matches answers. */
export type Script = [matches: string | RegExp, answer: Answer][];

/** The commands the page runs on every Android device, answered as a phone with nothing on it. */
export const ANDROID: Script = [
	["getprop ro.build.version.sdk", "35\n"],
	["dumpsys window displays", "mCurrentRotation=ROTATION_0\n"],
	["settings get system accelerometer_rotation", "1\n"],
	[/^su 0 id -u/, { exit: 1 }],
	[/^pm list packages/, ""],
	[/^cmd package query-activities/, ""],
	["ps -A -o NAME", "NAME\n"],
];

const text = new TextEncoder();
const words = new TextDecoder();

function bytes(data: string | Uint8Array): Uint8Array {
	return typeof data === "string" ? text.encode(data) : data;
}

class Reader {
	private held = new Uint8Array(0);

	constructor(private readonly channel: Channel) {}

	async exactly(length: number): Promise<Uint8Array | undefined> {
		while (this.held.byteLength < length) {
			const more = await Promise.race([
				this.channel.take(),
				this.channel.gone.then(() => undefined),
			]);

			if (more === undefined) {
				return undefined;
			}

			const joined = new Uint8Array(this.held.byteLength + more.byteLength);

			joined.set(this.held);
			joined.set(more, this.held.byteLength);
			this.held = joined;
		}

		const taken = this.held.slice(0, length);

		this.held = this.held.slice(length);

		return taken;
	}

	/** A 4-byte id and a little-endian u32, the header of a sync request. */
	async header(): Promise<{ id: string; length: number } | undefined> {
		const header = await this.exactly(8);

		if (!header) {
			return undefined;
		}

		return {
			id: words.decode(header.subarray(0, 4)),
			length: new DataView(header.buffer, header.byteOffset).getUint32(4, true),
		};
	}
}

// Shell protocol v2's packet ids.
const STDIN = 0;
const STDOUT = 1;
const STDERR = 2;
const EXIT = 3;
const CLOSE_STDIN = 4;
const WINDOW_SIZE = 5;

/** A shell v2 packet: an id, the length as 4 bytes little-endian, the data. */
function packet(id: number, data: Uint8Array): Uint8Array {
	const framed = new Uint8Array(5 + data.byteLength);

	framed[0] = id;
	new DataView(framed.buffer).setUint32(1, data.byteLength, true);
	framed.set(data, 5);

	return framed;
}

export class Process {
	size: { rows: number; cols: number } | undefined;
	private readonly input: (Uint8Array | undefined)[] = [];
	private readonly waiting: ((chunk: Uint8Array | undefined) => void)[] = [];
	private ended = false;

	constructor(
		private readonly adb: EventOf<"Adb">,
		readonly command: string,
		readonly terminal: string | undefined,
	) {
		void this.listen(new Reader(adb.channel));
	}

	get pty(): boolean {
		return this.terminal !== undefined;
	}

	read(): Promise<Uint8Array | undefined> {
		if (this.input.length > 0) {
			return Promise.resolve(this.input.shift());
		}

		return new Promise((took) => this.waiting.push(took));
	}

	async all(): Promise<Uint8Array> {
		const chunks: Uint8Array[] = [];

		for (let chunk = await this.read(); chunk; chunk = await this.read()) {
			chunks.push(chunk);
		}

		const joined = new Uint8Array(chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0));
		let at = 0;

		for (const chunk of chunks) {
			joined.set(chunk, at);
			at += chunk.byteLength;
		}

		return joined;
	}

	print(data: string | Uint8Array) {
		this.adb.channel.write(packet(STDOUT, bytes(data)));
	}

	/** On a pseudo-terminal adbd sends stderr as stdout, as there is one stream. */
	complain(data: string | Uint8Array) {
		this.adb.channel.write(packet(this.pty ? STDOUT : STDERR, bytes(data)));
	}

	exit(code = 0) {
		if (this.ended) {
			return;
		}

		this.ended = true;
		this.adb.channel.write(packet(EXIT, new Uint8Array([code])));
		this.adb.channel.end();
	}

	private give(chunk: Uint8Array | undefined) {
		const took = this.waiting.shift();

		if (took) {
			took(chunk);
		} else {
			this.input.push(chunk);
		}
	}

	private async listen(reader: Reader) {
		for (;;) {
			const header = await reader.exactly(5);

			if (!header) {
				this.give(undefined);
				return;
			}

			const length = new DataView(header.buffer, header.byteOffset).getUint32(1, true);
			const data = (await reader.exactly(length)) ?? new Uint8Array(0);

			if (header[0] === STDIN) {
				this.give(data);
			} else if (header[0] === CLOSE_STDIN) {
				this.give(undefined);
			} else if (header[0] === WINDOW_SIZE) {
				// "<rows>x<cols>,<x pixels>x<y pixels>" and a NUL.
				const [rows, cols] = words.decode(data).split(",")[0].split("x").map(Number);

				this.size = { rows, cols };
			}
		}
	}
}

export function shellService(service: string): { command: string; terminal?: string } | undefined {
	const found = /^shell(,[^:]*)?:(.*)$/s.exec(service);

	if (!found) {
		return undefined;
	}

	const options = (found[1] ?? "").split(",");
	const terminal = options.includes("pty")
		? (options.find((option) => option.startsWith("TERM="))?.slice(5) ?? "")
		: undefined;

	return { command: found[2], terminal };
}

export function command(service: string): string | undefined {
	return shellService(service)?.command;
}

async function run(adb: EventOf<"Adb">, answer: Answer) {
	const { command: line, terminal } = shellService(adb.service) ?? { command: "" };
	const process = new Process(adb, line, terminal);
	const said = typeof answer === "function" ? await answer(line, process) : answer;
	const { stdout, stderr, exit = 0 } = typeof said === "string" ? { stdout: said } : (said ?? {});

	if (stdout) {
		process.print(stdout);
	}

	if (stderr) {
		process.complain(stderr);
	}

	process.exit(exit);
}

async function sync(adb: EventOf<"Adb">, files: Map<string, Uint8Array>) {
	const reader = new Reader(adb.channel);
	const reply = (id: string, data: Uint8Array = new Uint8Array(0), length = data.byteLength) => {
		const framed = new Uint8Array(8 + data.byteLength);

		framed.set(text.encode(id));
		new DataView(framed.buffer).setUint32(4, length, true);
		framed.set(data, 8);
		adb.channel.write(framed);
	};

	for (;;) {
		const request = await reader.header();

		if (!request || request.id === "QUIT") {
			adb.channel.end();
			return;
		}

		const argument = words.decode((await reader.exactly(request.length)) ?? new Uint8Array(0));

		if (request.id === "SEND") {
			// "<path>,<mode>", then DATA chunks to a DONE that carries the time.
			const path = argument.slice(0, argument.lastIndexOf(","));
			const chunks: Uint8Array[] = [];

			for (;;) {
				const part = await reader.header();

				if (!part) {
					return;
				}

				if (part.id === "DONE") {
					break;
				}

				chunks.push((await reader.exactly(part.length)) ?? new Uint8Array(0));
			}

			const file = new Uint8Array(chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0));
			let at = 0;

			for (const chunk of chunks) {
				file.set(chunk, at);
				at += chunk.byteLength;
			}

			files.set(path, file);
			reply("OKAY", undefined, 0);
		} else if (request.id === "RECV") {
			const file = files.get(argument);

			if (!file) {
				reply("FAIL", text.encode("open failed: No such file or directory"));
				continue;
			}

			// adbd sends a file in chunks of at most 64 KiB.
			for (let at = 0; at < file.byteLength; at += 64 * 1024) {
				reply("DATA", file.subarray(at, at + 64 * 1024));
			}

			reply("DONE", undefined, 0);
		} else {
			reply("FAIL", text.encode(`sync failed: ${request.id} is not played by the test`));
		}
	}
}

export class Phone {
	readonly ran: string[] = [];
	readonly files = new Map<string, Uint8Array>();
	readonly stop: () => void;

	constructor(
		tetherd: Tetherd,
		readonly id: DeviceId,
		private readonly script: Script,
	) {
		const on = line(id);
		const shells = tetherd.each(
			"Adb",
			(event) => event.device === on && this.answer(event.service) !== undefined,
			(adb) => {
				this.ran.push(command(adb.service) ?? "");
				adb.open();
				void run(adb, this.answer(adb.service) as Answer);
			},
		);
		const syncs = tetherd.each(
			"Adb",
			(event) => event.device === on && event.service === "sync:",
			(adb) => {
				adb.open();
				void sync(adb, this.files);
			},
		);

		this.stop = () => {
			shells();
			syncs();
		};
	}

	/** Answers ahead of the earlier answers. */
	on(matches: string | RegExp, answer: Answer) {
		this.script.unshift([matches, answer]);
	}

	private answer(service: string): Answer | undefined {
		const run = command(service);

		if (run === undefined) {
			return undefined;
		}

		return (
			this.script.find(([matches]) =>
				typeof matches === "string" ? run === matches : matches.test(run),
			)?.[1] ?? this.reading(run)
		);
	}

	/**
	 * Reads one of its files, as `cat <path>` does, or as the script that reads
	 * a file or tars a folder does (`read_script` in crates/android/src/files.rs).
	 */
	private reading(run: string): Answer | undefined {
		const read =
			/^cat ('[^']*'|\S+)$/.exec(run) ??
			/^if \[ -d ('[^']*'|\S+) \]; then tar -cf - -C \S+ \S+; else cat \1; fi$/.exec(run);

		if (!read) {
			return undefined;
		}

		const path = read[1].replace(/^'(.*)'$/, "$1");
		const file = this.files.get(path);

		return file
			? { stdout: file }
			: { stderr: `cat: ${path}: No such file or directory\n`, exit: 1 };
	}
}

/**
 * A command neither `script` nor `ANDROID` answers stays queued for the test,
 * and fails it at its end if nothing took it.
 */
export function phone(tetherd: Tetherd, id: DeviceId, script: Script = []): Phone {
	return new Phone(tetherd, id, [...script, ...ANDROID]);
}

export async function shell(
	tetherd: Tetherd,
	device: DeviceId,
	matches: string | RegExp,
	answer: Answer,
): Promise<EventOf<"Adb">> {
	const adb = await tetherd.next("Adb", (event) => {
		const run = command(event.service);

		return (
			event.device === line(device) &&
			run !== undefined &&
			(typeof matches === "string" ? run === matches : matches.test(run))
		);
	});

	adb.open();
	void run(adb, answer);

	return adb;
}

export function reply(adb: EventOf<"Adb">, ran: Ran) {
	void run(adb, ran);
}
