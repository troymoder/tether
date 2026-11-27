import type { EmulatorSettings, Feeder, Lens } from "./protocol";
import { emulator, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, noScreens, phone, topic } from "./helpers";

const booted = (tetherd: Parameters<typeof noScreens>[0]) => {
	const pixel = emulator("pixel", { status: "Booted" });

	phone(tetherd, pixel.id);
	noScreens(tetherd);

	return pixel;
};

function settings(lens: Lens = "Back"): EmulatorSettings {
	return {
		profile: "pixel_6",
		image: "system-images;android-36;google_apis;x86_64",
		memory: 4096,
		graphics: "Software",
		autostart: false,
		camera: { lens, settings: { width: 640, height: 480, fps: 30 } },
	};
}

function feeder(name: string): Feeder {
	return { print: `SHA256:${name}`, name, since: 0 };
}

test("the screen button shares a screen after the file source was open", async ({
	tetherd,
	tether,
}) => {
	const pixel = booted(tetherd);
	const chooser: string[] = [];
	const asked = () => tether.page.evaluate(() => (window as any).screens as number);

	await tether.page.addInitScript(() => {
		(window as any).screens = 0;
		navigator.mediaDevices.getDisplayMedia = () => {
			(window as any).screens++;

			return Promise.reject(new DOMException("refused", "NotAllowedError"));
		};
	});
	tether.page.on("filechooser", () => chooser.push("opened"));
	// The page logs the refused screen as an event, and asks for the webcams
	// of tetherd's machine, which this test doesn't look at.
	tetherd.ignore("Event");
	tetherd.ignore("Lane", { job: "Webcams" });
	await tether.open(`${route(pixel.id)}/camera`);
	await connect(tetherd, { devices: [pixel] });
	await tether.ready();
	await topic(
		tetherd,
		{ Emulator: "pixel" },
		{ Emulator: { name: "pixel", settings: settings() } },
	);

	await tether.page.getByRole("button", { name: /^What the device's camera is fed/ }).click();
	const sources = tether.page.getByRole("listbox", { name: "Source" });

	await sources.getByRole("option", { name: /^File/ }).first().click();
	await expect.poll(() => chooser.length).toBe(1);
	await sources.getByRole("option", { name: /^Screen/ }).click();
	await expect.poll(asked).toBe(1);

	await tether.page.getByRole("button", { name: "Share a screen or a window" }).click();
	await tether.page.waitForTimeout(500);
	expect(
		{ screens: await asked(), choosers: chooser.length },
		"the press asks for one screen and opens no file chooser",
	).toEqual({ screens: 2, choosers: 1 });
});

test("take over asks about the browser that feeds the camera now", async ({ tetherd, tether }) => {
	const pixel = booted(tetherd);
	const fedBy = (name: string) => ({
		...pixel,
		emulator: { ...pixel.emulator!, fed_by: feeder(name) },
	});

	await tether.open(`${route(fedBy("Firefox").id)}/camera`);
	await connect(tetherd, { devices: [fedBy("Firefox")] });
	await tether.ready();
	await topic(
		tetherd,
		{ Emulator: "pixel" },
		{ Emulator: { name: "pixel", settings: settings() } },
	);
	await expect(tether.page.getByText("In use by Firefox since")).toBeVisible();

	tetherd.send({ Device: fedBy("Safari") });
	await expect(tether.page.getByText("In use by Safari since")).toBeVisible();

	await tether.page.getByRole("button", { name: "Take over" }).click();
	const dialog = tether.page.getByRole("alertdialog");

	await expect(dialog).toContainText("Safari stops feeding it.");
	await dialog.getByRole("button", { name: "Take over" }).click();
	(await tetherd.next("Call", { request: { TakeCamera: { emulator: "pixel" } } })).reply({
		Ok: "Took the camera",
	});
	await tether.page.waitForTimeout(300);
	expect(
		tetherd.left().filter((event) => event.kind === "Call"),
		"one take per press",
	).toEqual([]);
});

test("start it feeds the lens the emulator has now", async ({ tetherd, tether }) => {
	const pixel = booted(tetherd);

	await tether.page.addInitScript(() => {
		(window as any).allow = false;
		navigator.mediaDevices.getUserMedia = async () => {
			if (!(window as any).allow) {
				throw new DOMException("refused", "NotAllowedError");
			}

			const canvas = document.createElement("canvas");

			canvas.width = 64;
			canvas.height = 48;
			canvas.getContext("2d")!.fillRect(0, 0, 64, 48);

			return canvas.captureStream(30);
		};
	});
	tetherd.ignore("Event");
	tetherd.ignore("Lane", { job: "Webcams" });
	await tether.open(`${route(pixel.id)}/camera`);
	await connect(tetherd, { devices: [pixel] });
	await tether.ready();
	await topic(
		tetherd,
		{ Emulator: "pixel" },
		{ Emulator: { name: "pixel", settings: settings("Back") } },
	);

	await tether.page.getByRole("button", { name: /^What the device's camera is fed/ }).click();
	await tether.page
		.getByRole("listbox", { name: "Source" })
		.getByRole("option", { name: /^Webcam/ })
		.first()
		.click();
	await expect(tether.page.getByRole("button", { name: "Start it" })).toBeVisible();

	tetherd.send({ Value: { Emulator: { name: "pixel", settings: settings("Front") } } });
	await expect(tether.page.getByText(/What the front camera/)).toBeVisible();
	// The source control starts over for the moved camera.
	await tether.page.getByRole("button", { name: /^What the device's camera is fed/ }).click();
	await tether.page
		.getByRole("listbox", { name: "Source" })
		.getByRole("option", { name: /^Webcam/ })
		.first()
		.click();

	await tether.page.evaluate(() => {
		(window as any).allow = true;
	});
	await tether.page.getByRole("button", { name: "Start it" }).click();

	const lane = await tetherd.next("Lane", { media: { Camera: { emulator: "pixel" } } });

	expect(lane.media).toEqual({ Camera: { emulator: "pixel", lens: "Front", lossless: false } });
	tetherd.ignore("Lane");
	tetherd.ignore("Call");
});
