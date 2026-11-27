import { android, emulator, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, ignoreTopic, noScreens, ok, phone } from "./helpers";

test("disconnect asks tetherd to disconnect the picked phone", async ({ tetherd, tether }) => {
	const pixel8 = android("R58M", { name: "Pixel 8" });

	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	await tether.page.getByRole("button", { name: "Disconnect Pixel 8" }).click();

	const call = await tetherd.next("Call", { request: { Disconnect: { device: pixel8.id } } });

	call.reply(ok("Pixel 8 disconnected"));
	await expect(tether.toast()).toHaveText("Pixel 8 disconnected");
});

test("disconnect works on a phone picked after the page opened", async ({ tetherd, tether }) => {
	const pixel8 = android("R58M", { name: "Pixel 8" });

	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	// The page opens on no device, so the button starts out with nothing to do.
	await tether.open("/devices");
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	await tether.card("Pixel 8").click();
	await tether.page.getByRole("button", { name: "Disconnect Pixel 8" }).click();

	(await tetherd.next("Call", { request: { Disconnect: { device: pixel8.id } } })).reply(
		ok("Pixel 8 disconnected"),
	);
	await expect(tether.toast()).toHaveText("Pixel 8 disconnected");
});

test("the power button follows a device picked from its card", async ({ tetherd, tether }) => {
	const pixel8 = android("R58M", { name: "Pixel 8" });
	const pixel = emulator("pixel");

	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8, pixel] });
	await tether.ready();
	await expect(tether.page.getByRole("button", { name: "Disconnect Pixel 8" })).toBeVisible();

	await tether.card("pixel").click();
	await tether.page.getByRole("button", { name: "Start pixel" }).click();

	const call = await tetherd.next("Call", { request: { Start: { name: "pixel" } } });

	call.reply(ok("pixel is starting"));
	await expect(tether.toast()).toHaveText("pixel is starting");

	ignoreTopic(tetherd, { Emulator: "pixel" });
	// The page shows it booting at once and watches for its pictures.
	tetherd.ignore("Lane", { job: "Watch" });
});

test("the power button follows a device picked in the picker", async ({ tetherd, tether }) => {
	const pixel8 = android("R58M", { name: "Pixel 8" });
	const pixel = emulator("pixel", { status: "Booted" });

	phone(tetherd, pixel8.id);
	phone(tetherd, pixel.id);
	noScreens(tetherd);
	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8, pixel] });
	await tether.ready();
	await expect(tether.page.getByRole("button", { name: "Disconnect Pixel 8" })).toBeVisible();

	await tether.page.getByRole("button", { name: /^Device:/ }).click();
	await tether.page.getByRole("option", { name: /pixel/ }).click();
	await tether.page.getByRole("button", { name: "Stop pixel" }).click();

	(await tetherd.next("Call", { request: { Stop: { name: "pixel" } } })).reply(
		ok("pixel is stopping"),
	);
	await expect(tether.toast()).toHaveText("pixel is stopping");

	ignoreTopic(tetherd, { Emulator: "pixel" });
	// The page shows it stopping at once and watches for its pictures.
	tetherd.ignore("Lane", { job: "Watch" });
});

test("a phone that is only available is connected through tetherd", async ({ tetherd, tether }) => {
	const way = { means: "Wifi", at: { Instance: "SHA256:primary" } } as const;
	const pixel8 = android("R58M", {
		name: "Pixel 8",
		condition: "Available",
		routes: [{ way, address: "10.0.0.8:5555", pairing: false }],
	});

	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	await tether.page.getByRole("button", { name: "Connect to Pixel 8" }).click();

	const call = await tetherd.next("Call", { request: { Connect: { device: pixel8.id } } });

	expect(call.request).toMatchObject({ Connect: { take: false } });
	call.reply(ok("Connecting to Pixel 8"));
	await expect(tether.toast()).toHaveText("Connecting to Pixel 8");
});

test("an unavailable device greys the button", async ({ tetherd, tether }) => {
	const pixel8 = android("R58M", { name: "Pixel 8", condition: "Unavailable", routes: [] });

	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	await expect(tether.page.getByRole("button", { name: "Pixel 8 is unavailable" })).toBeDisabled();
});

test("the button turns to connect once tetherd says the phone went", async ({
	tetherd,
	tether,
}) => {
	const pixel8 = android("R58M", { name: "Pixel 8" });

	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();
	await expect(tether.page.getByRole("button", { name: "Disconnect Pixel 8" })).toBeVisible();

	tetherd.send({ Device: { ...pixel8, condition: "Available" } });
	await expect(tether.page.getByRole("button", { name: "Connect to Pixel 8" })).toBeEnabled();
});

test("a phone connected after the page opened is disconnected, not connected again", async ({
	tetherd,
	tether,
}) => {
	const ready = android("R58M", { name: "Pixel 8" });
	const pixel8 = { ...ready, condition: "Available" as const };

	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	await tether.page.getByRole("button", { name: "Connect to Pixel 8" }).click();
	(await tetherd.next("Call", { request: { Connect: { device: pixel8.id } } })).reply(
		ok("Connected to Pixel 8"),
	);
	tetherd.send({ Device: ready });

	await tether.page.getByRole("button", { name: "Disconnect Pixel 8" }).click();
	(await tetherd.next("Call", { request: { Disconnect: { device: pixel8.id } } })).reply(
		ok("Disconnected from Pixel 8"),
	);
	await expect(tether.toast()).toHaveText("Disconnected from Pixel 8");
});

test("the side button hides and shows the device each time it is pressed", async ({
	tetherd,
	tether,
}) => {
	const side = tether.page.getByRole("button", { name: /^(Hide|Show) the device$/ });

	await tether.open();
	await connect(tetherd);
	await tether.ready();

	for (const pressed of ["true", "false", "true", "false"]) {
		await side.click();
		await expect(side).toHaveAttribute("aria-pressed", pressed);
	}
});
