import { android, emulator, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, ignoreTopic, phone } from "./helpers";
import { SURFACE, heard, hears, screen, shows, watch } from "./screen";

const pixel8 = android("R58M", { name: "Pixel 8" });

test("the page shows what the device's screen shows", async ({ tetherd, tether }) => {
	phone(tetherd, pixel8.id);
	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	const played = await screen(tetherd);

	await played.paint("rgb(220, 30, 30)");
	await shows(tether.page, [220, 30, 30]);

	await played.paint("rgb(30, 60, 220)");
	await shows(tether.page, [30, 60, 220]);
});

test("a tap on the screen reaches the device where it landed", async ({ tetherd, tether }) => {
	phone(tetherd, pixel8.id);
	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	const played = await screen(tetherd);

	await played.paint("white");
	await shows(tether.page, [255, 255, 255]);
	await tether.page.locator(SURFACE).click();

	const down = await played.touch();
	const up = await played.touch();

	expect([down.action, up.action]).toEqual(["down", "up"]);
	expect(down.width).toBe(180);
	expect(down.height).toBe(320);
	expect(Math.abs(down.x - 90)).toBeLessThanOrEqual(2);
	expect(Math.abs(down.y - 160)).toBeLessThanOrEqual(2);
});

test("the page hears the device's sound, and its silence", async ({ tetherd, tether }) => {
	phone(tetherd, pixel8.id);
	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	const played = await screen(tetherd);

	await played.tone(440);
	await expect.poll(() => hears(tether.page, 440)).toBe(true);

	await played.tone(1000);
	await expect.poll(() => hears(tether.page, 1000)).toBe(true);

	await played.silence();
	await expect.poll(() => heard(tether.page)).toBe(0);
});

test("a booting emulator's pictures show until its screen streams", async ({ tetherd, tether }) => {
	const booting = emulator("pixel", { status: "Booting" });

	await tether.open(`${route(booting.id)}/devices`);
	await connect(tetherd, { devices: [booting] });
	await tether.ready();
	ignoreTopic(tetherd, { Emulator: "pixel" });

	const watched = await watch(tetherd, booting.id);

	watched.picture([20, 160, 60]);
	await shows(tether.page, [20, 160, 60], "picture");

	watched.picture([200, 120, 20]);
	await shows(tether.page, [200, 120, 20], "picture");

	// Booted, it streams like any device, and the pictures stop.
	phone(tetherd, booting.id);
	tetherd.send({ Device: emulator("pixel", { status: "Booted" }) });
	await watched.lane.channel.gone;

	const played = await screen(tetherd);

	await played.paint("rgb(30, 60, 220)");
	await shows(tether.page, [30, 60, 220]);
});
