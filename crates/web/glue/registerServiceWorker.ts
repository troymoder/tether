import type { Answer, Request } from "./serviceWorker/messages";

type Read = (offset: number, length: number) => Promise<Uint8Array>;

interface Inflight {
	size: number;
	type: string;
	read: Read;
}

const inflight = new Map<string, Inflight>();

function answer({ data, ports: [port] }: MessageEvent<Request>) {
	if (!port) {
		return;
	}

	const stream = inflight.get(data?.token);

	if (!stream) {
		port.postMessage({ error: "closed" });
		return;
	}

	stream.read(data.offset, data.length).then(
		(bytes) =>
			port.postMessage({ size: stream.size, type: stream.type, bytes } satisfies Answer, [
				bytes.buffer,
			]),
		(error) => port.postMessage({ error: String(error) } satisfies Answer),
	);
}

/** Browsers offer Service Workers only in a secure context. Without one, such
 * as over plain http to another computer, the Media view reads each file whole. */
async function register(): Promise<boolean> {
	if (!("serviceWorker" in navigator)) {
		return false;
	}

	navigator.serviceWorker.addEventListener("message", answer);

	try {
		// rspack bundles the worker on its own when it sees this exact expression.
		await navigator.serviceWorker.register(new URL("./serviceWorker/index.ts", import.meta.url), {
			type: "module",
		});
		await navigator.serviceWorker.ready;
		return true;
	} catch (error) {
		console.warn(`stream failed: ${error}`);
		return false;
	}
}

const registered = register();

/** A load that skipped the worker, such as a hard reload, has no controller. */
async function ready(): Promise<boolean> {
	return (await registered) && navigator.serviceWorker.controller !== null;
}

function open(size: number, type: string, read: Read): string {
	const token = crypto.randomUUID();

	inflight.set(token, { size, type, read });

	return `media-stream/${token}`;
}

function close(address: string) {
	inflight.delete(address.slice("media-stream/".length));
}

Object.assign(globalThis, { tetherStream: { ready, open, close } });
