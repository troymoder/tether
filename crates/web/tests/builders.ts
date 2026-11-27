import type { Access, App, Device, DeviceId, Place, Status } from "./protocol";
import { ORIGIN } from "./tetherd";

export const PRIMARY = "SHA256:primary";

export const YOU = "SHA256:test";

/** A device's id as one line, as `DeviceId`'s Display writes it. */
export function line(id: DeviceId): string {
	if ("Android" in id) {
		return `android:${id.Android}`;
	}

	if ("Iphone" in id) {
		return `iphone:${id.Iphone}`;
	}

	return `emulator:${id.Emulator.port}@${id.Emulator.instance}`;
}

/** The page's address part for a device, as app/route.rs writes it. */
export function route(id: DeviceId): string {
	return encodeURIComponent(line(id));
}

export function access(more: Partial<Access> = {}): Access {
	return {
		primary: PRIMARY,
		trusted: [{ print: YOU, name: "Chromium on Linux", kind: "Browser", at: 0 }],
		asking: [],
		instances: [{ id: PRIMARY, name: "test" }],
		...more,
	};
}

export function iphone(udid: string, more: Partial<Device> = {}): Device {
	const way = { means: "Usb", at: { Instance: PRIMARY } } as const;

	return {
		id: { Iphone: udid },
		name: udid,
		condition: {
			Connected: {
				way,
				state: "Ready",
				described: { release: "26.1", abis: [] },
			},
		},
		routes: [{ way, address: udid, pairing: false }],
		last: { way, at: 0 },
		emulator: undefined,
		...more,
	};
}

export function android(serial: string, more: Partial<Device> = {}): Device {
	const way = { means: "Usb", at: { Instance: PRIMARY } } as const;

	return {
		id: { Android: serial },
		name: serial,
		condition: {
			Connected: {
				way,
				state: "Ready",
				described: { release: "15", abis: ["arm64-v8a"] },
			},
		},
		routes: [{ way, address: serial, pairing: false }],
		last: { way, at: 0 },
		emulator: undefined,
		...more,
	};
}

export function emulator(
	avd: string,
	{ status = "Shutdown", port = 5554 }: { status?: Status; port?: number } = {},
	more: Partial<Device> = {},
): Device {
	const way = { means: "Emulator", at: { Instance: PRIMARY } } as const;
	const up = status === "Booted";

	return {
		id: { Emulator: { port, instance: PRIMARY } },
		name: avd,
		condition: up
			? {
					Connected: {
						way,
						state: "Ready",
						described: { release: "15", abis: ["x86_64"] },
					},
				}
			: "Unavailable",
		routes: up ? [{ way, address: `emulator-${port}`, pairing: false }] : [],
		last: { way, at: 0 },
		emulator: {
			avd,
			status,
			upright: true,
			camera: undefined,
			fed_by: undefined,
			remote: undefined,
		},
		...more,
	};
}

export function app(id: string, more: Partial<App> = {}): App {
	return {
		id,
		name: id,
		icon: undefined,
		builds: 1,
		variants: [
			{
				abis: [],
				build: 1,
				version: "1.0",
				handed_over: 0,
				size: 1024,
				hash: "0".repeat(64),
				file: `${id}-1.apk`,
			},
		],
		auto: [],
		...more,
	};
}

export function place(name: string, latitude = 51.5, longitude = -0.12): Place {
	return { name, latitude, longitude };
}

type Table = {
	name: string;
	autoIncrement: boolean;
	records: { key: string; value: string }[];
	indexes: [];
};

/**
 * Playwright restores IndexedDB from a storage state it wrote, though its
 * types leave the field out.
 */
export type Stored = {
	cookies: [];
	origins: {
		origin: string;
		localStorage: [];
		indexedDB: { name: string; version: number; stores: Table[] }[];
	}[];
};

/**
 * It is the IndexedDB database app/storage.rs makes, at its version, with
 * every table, so the page finds it as it left it. The browser's key pair is
 * left out, because a CryptoKey cannot be serialized, and the page makes a
 * new one as on a first visit.
 */
export function stored(preferences: Record<string, string>): Stored {
	const table = (name: string, records: Table["records"] = []): Table => ({
		name,
		autoIncrement: false,
		records,
		indexes: [],
	});

	return {
		cookies: [],
		origins: [
			{
				origin: ORIGIN,
				localStorage: [],
				indexedDB: [
					{
						name: "tether",
						version: 7,
						stores: [
							table("keys"),
							table(
								"preferences",
								Object.entries(preferences).map(([key, value]) => ({ key, value })),
							),
							table("cache"),
							table("cache_counts"),
						],
					},
				],
			},
		],
	};
}
