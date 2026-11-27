import type { Access, App, Device, Topic, Value } from "./protocol";
import { YOU, access as primaryAccess } from "./builders";
import { encodeScreen } from "./protocol";
import type { Tetherd } from "./tetherd";

export type State = {
	devices?: Device[];
	access?: Access;
	you?: string;
};

export async function connect(tetherd: Tetherd, state: State = {}) {
	await (await tetherd.next("Hello")).reply();
	await (await tetherd.next("Connect")).admit(state.you ?? YOU);
	await topic(tetherd, "Devices", { Devices: state.devices ?? [] });
	await topic(tetherd, "Access", { Access: state.access ?? primaryAccess() });
	await tetherd.next("Usb");
}

/** For tests that show a ready device without looking at its screen. */
export function noScreens(tetherd: Tetherd): () => void {
	const offers = tetherd.each("Offer", undefined, (offer) => void offer.answer());
	const lanes = tetherd.each("Lane", { job: "Screen" }, (lane) => {
		lane.channel.send(encodeScreen({ Failed: "screen failed: not in this test" }));
		lane.channel.end();
	});
	// The page reports the screen that failed above.
	const failed = tetherd.each("Event", { what: "screen", failed: true }, () => {});

	return () => {
		offers();
		lanes();
		failed();
	};
}

export async function topic(tetherd: Tetherd, name: Topic, value: Value) {
	await tetherd.next("Subscribe", { topic: name });
	(await tetherd.next("Query", { topic: name })).send(value);
}

export function ignoreTopic(tetherd: Tetherd, name: Topic) {
	tetherd.ignore("Subscribe", { topic: name });
	tetherd.ignore("Query", { topic: name });
	tetherd.ignore("Unsubscribe", { topic: name });
}

export function apps(tetherd: Tetherd, list: App[]) {
	return topic(tetherd, "Apps", { Apps: list });
}

/** `ls -lA` output for these entries, as the Files view lists a folder. */
export function listing(entries: { name: string; folder?: boolean; size?: number }[]): string {
	const lines = entries.map(({ name, folder = false, size = 0 }) => {
		const mode = folder ? "drwxrwx--x" : "-rw-rw----";

		return `${mode} 2 root everybody ${size} 2026-11-25 12:00 ${name}`;
	});

	return `total ${entries.length}\n${lines.join("\n")}\n`;
}

export {
	ANDROID,
	type Answer,
	Phone,
	Process,
	type Ran,
	type Script,
	command,
	phone,
	reply,
	shell,
} from "./adb";
export { ok, err } from "./tetherd";
