// The page fetches the helper's dex from tetherd on an `AndroidHelper` lane,
// pushes it over sync and starts it with app_process. The framing is that of
// crates/android/src/helper.rs. A request is its id (u32), kind (u8) and a
// count (u32) of length-prefixed strings. A reply is the id, a status byte
// (0 done, 1 failed), then the reason, or a count of records, each a count of
// length-prefixed fields. Numbers are big-endian.

import type { Phone, Process } from "./adb";
import { encodeFile } from "./protocol";
import type { Tetherd } from "./tetherd";

/** The path the page pushes the helper to (crates/android/src/helper.rs). */
export const DEX = "/data/local/tmp/tether-helper.dex";

/** Any bytes do, because nothing runs them. */
const BUILT = new TextEncoder().encode("dex\n035\0tether helper played by the test");

/** The kind of a request, as Main.kt numbers it. */
export enum Kind {
	Apps = 1,
	Thumbnails = 2,
	Count = 3,
	Page = 4,
	Read = 5,
}

export type App = { label: string; version?: string; icon?: Uint8Array };

export type Medium = {
	/** Its real path, under `/storage/emulated/0`, as MediaStore has it. */
	path: string;
	/** In seconds since 1970. Newer comes first. */
	changed?: number;
	video?: boolean;
	thumbnail?: Uint8Array;
};

const text = new TextEncoder();
const words = new TextDecoder();

function number(value: number): Uint8Array {
	const bytes = new Uint8Array(4);

	new DataView(bytes.buffer).setUint32(0, value);

	return bytes;
}

function joined(chunks: Uint8Array[]): Uint8Array {
	const whole = new Uint8Array(chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0));
	let at = 0;

	for (const chunk of chunks) {
		whole.set(chunk, at);
		at += chunk.byteLength;
	}

	return whole;
}

function field(bytes: Uint8Array): Uint8Array {
	return joined([number(bytes.byteLength), bytes]);
}

type Record = Uint8Array[];

function reply(id: number, records: Record[] | Error): Uint8Array {
	if (records instanceof Error) {
		return joined([number(id), new Uint8Array([1]), field(text.encode(records.message))]);
	}

	return joined([
		number(id),
		new Uint8Array([0]),
		number(records.length),
		...records.flatMap((record) => [number(record.length), ...record.map(field)]),
	]);
}

/** One app or thumbnail: its id, the reason it failed or nothing, label, version and image. */
function item(
	id: string,
	reason: string,
	label = "",
	version = "",
	image: Uint8Array = new Uint8Array(0),
): Record {
	return [...[id, reason, label, version].map((part) => text.encode(part)), image];
}

export class Helper {
	readonly apps = new Map<string, App>();
	readonly media: Medium[] = [];
	readonly asked: { kind: Kind; arguments: string[] }[] = [];

	constructor(private readonly phone: Phone) {}

	add(medium: Medium, file: Uint8Array = new Uint8Array(1024)) {
		this.media.push(medium);
		this.phone.files.set(medium.path, file);
	}

	async run(process: Process) {
		const pushed = this.phone.files.get(DEX);

		if (!pushed || words.decode(pushed) !== words.decode(BUILT)) {
			process.complain("Error: Could not find or load main class tether.Main\n");
			process.exit(1);
			return;
		}

		let held: Uint8Array = new Uint8Array(0);
		const take = async (length: number): Promise<Uint8Array | undefined> => {
			while (held.byteLength < length) {
				const more = await process.read();

				if (!more) {
					return undefined;
				}

				held = joined([held, more]);
			}

			const taken = held.slice(0, length);

			held = held.slice(length);

			return taken;
		};
		const u32 = async () => {
			const bytes = await take(4);

			return bytes && new DataView(bytes.buffer, bytes.byteOffset).getUint32(0);
		};

		for (;;) {
			const id = await u32();
			const kind = await take(1);
			const count = await u32();

			if (id === undefined || !kind || count === undefined) {
				process.exit(0);
				return;
			}

			const arguments_: string[] = [];

			for (let at = 0; at < count; at++) {
				const length = await u32();
				const argument = length === undefined ? undefined : await take(length);

				if (!argument) {
					process.exit(0);
					return;
				}

				arguments_.push(words.decode(argument));
			}

			this.asked.push({ kind: kind[0], arguments: arguments_ });

			// Main.kt answers each request on a thread of its own. This answers
			// each at once, which is one of the orders the page has to handle.
			let answered: Record[] | Error;

			try {
				answered = this.answer(kind[0], arguments_);
			} catch (error) {
				answered = error as Error;
			}

			process.print(reply(id, answered));
		}
	}

	private answer(kind: Kind, asked: string[]): Record[] {
		switch (kind) {
			case Kind.Apps:
				return asked.map((id) => {
					const app = this.apps.get(id);

					if (!app) {
						return item(id, `android.content.pm.PackageManager$NameNotFoundException: ${id}`);
					}

					return item(id, "", app.label, app.version ?? "", app.icon);
				});
			case Kind.Thumbnails:
				return asked.map((path) => {
					const thumbnail = this.media.find((medium) => medium.path === path)?.thumbnail;

					if (!thumbnail) {
						return item(path, `java.io.FileNotFoundException: ${path}`);
					}

					return item(path, "", "", "", thumbnail);
				});
			case Kind.Count:
				return [[text.encode(String(this.within(asked).length))]];
			case Kind.Page: {
				const [offset, limit] = asked.slice(0, 2).map(Number);

				return this.within(asked.slice(2))
					.slice(offset, offset + limit)
					.map((medium) => {
						const id = this.media.indexOf(medium) + 1;
						const size = this.phone.files.get(medium.path)?.byteLength ?? 0;

						return [
							medium.path,
							String(size),
							String(medium.changed ?? 0),
							medium.video ? "3" : "1",
							String(id),
						].map((part) => text.encode(part));
					});
			}
			case Kind.Read: {
				const [path, offset, length] = asked;
				const file = this.phone.files.get(path);

				if (!file) {
					throw new Error(
						`java.io.FileNotFoundException: ${path}: open failed: ENOENT (No such file or directory)`,
					);
				}

				return [[file.slice(Number(offset), Number(offset) + Number(length))]];
			}
			default:
				throw new Error(`java.lang.IllegalArgumentException: unknown request ${kind}`);
		}
	}

	private within(folders: string[]): Medium[] {
		return this.media
			.filter((medium) => folders.includes(medium.path.slice(0, medium.path.lastIndexOf("/"))))
			.sort(
				(a, b) =>
					(b.changed ?? 0) - (a.changed ?? 0) || this.media.indexOf(b) - this.media.indexOf(a),
			);
	}
}

export function helper(tetherd: Tetherd, phone: Phone): Helper {
	const played = new Helper(phone);

	tetherd.each("Lane", { job: "AndroidHelper" }, (lane) => {
		lane.channel.send(encodeFile(BUILT));
		lane.channel.end();
	});
	phone.on(/^CLASSPATH=\S+ app_process \/ tether\.Main$/, async (_, process) => {
		await played.run(process);
	});

	return played;
}
