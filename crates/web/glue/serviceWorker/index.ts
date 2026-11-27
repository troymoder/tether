// A fetch to media-stream/<token> is answered with bytes the page that opened
// the token reads from the device, a slice at a time, so a video plays and
// seeks without the whole file being downloaded first.
//
// It is type-checked on its own (serviceWorker/tsconfig.json), because the
// webworker and dom libraries cannot be in one program.

import type { Answer, Request } from "./messages";

const worker = self as unknown as ServiceWorkerGlobalScope;

// The most bytes one message carries. A range answer is cut to this size, so
// a seek only reads the bytes near where it lands.
const CHUNK = 1024 * 1024;

const PREFIX = "/media-stream/";

worker.addEventListener("install", () => worker.skipWaiting());

// The page that registered this is controlled at once, without a reload.
worker.addEventListener("activate", (event) => event.waitUntil(worker.clients.claim()));

worker.addEventListener("fetch", (event) => {
	const url = new URL(event.request.url);
	const at = url.pathname.lastIndexOf(PREFIX);

	if (url.origin !== worker.location.origin || at < 0) {
		return;
	}

	const token = url.pathname.slice(at + PREFIX.length);

	event.respondWith(
		answer(event, token).catch(
			(error) => new Response(`stream failed: ${error?.message ?? error}`, { status: 502 }),
		),
	);
});

type Read = Extract<Answer, { bytes: unknown }>;

function ask(client: Client, token: string, offset: number, length: number): Promise<Read> {
	const { port1, port2 } = new MessageChannel();

	return new Promise<Read>((resolve, reject) => {
		port1.onmessage = ({ data }: MessageEvent<Answer>) => {
			port1.close();
			"error" in data ? reject(new Error(data.error)) : resolve(data);
		};

		client.postMessage({ token, offset, length } satisfies Request, [port2]);
	});
}

async function answer(event: FetchEvent, token: string): Promise<Response> {
	const client = await worker.clients.get(event.clientId);

	if (!client) {
		throw new Error("no page");
	}

	const range = /^bytes=(\d+)-(\d*)$/.exec(event.request.headers.get("Range") ?? "");

	if (!range) {
		return whole(client, token);
	}

	const start = Number(range[1]);
	const end = range[2] ? Number(range[2]) + 1 : Infinity;
	const { size, type, bytes } = await ask(client, token, start, Math.min(end - start, CHUNK));

	if (start >= size) {
		return new Response(null, { status: 416, headers: { "Content-Range": `bytes */${size}` } });
	}

	// A message arrives as a structured clone, which always has its own ArrayBuffer.
	return new Response(bytes as Uint8Array<ArrayBuffer>, {
		status: 206,
		headers: {
			"Accept-Ranges": "bytes",
			"Content-Length": String(bytes.byteLength),
			"Content-Range": `bytes ${start}-${start + bytes.byteLength - 1}/${size}`,
			"Content-Type": type,
		},
	});
}

// An <img> fetches without a range and, unlike a <video>, never asks for the
// rest of a partial answer, so it gets the whole file.
async function whole(client: Client, token: string): Promise<Response> {
	const { size, type, bytes: first } = await ask(client, token, 0, CHUNK);
	let offset = 0;

	const body = new ReadableStream<Uint8Array>({
		async pull(controller) {
			const bytes = offset === 0 ? first : (await ask(client, token, offset, CHUNK)).bytes;

			if (bytes.byteLength === 0 && offset < size) {
				controller.error(new Error("file ended early"));
				return;
			}

			offset += bytes.byteLength;
			controller.enqueue(bytes);

			if (offset >= size) {
				controller.close();
			}
		},
	});

	return new Response(body, {
		headers: {
			"Accept-Ranges": "bytes",
			"Content-Length": String(size),
			"Content-Type": type,
		},
	});
}
