// The behaviour under test is in link/pending.rs. Each test takes tetherd's
// call and holds its answer to check the page in between.

import { android, emulator, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, err, ignoreTopic, noScreens, ok, phone } from "./helpers";
import type { Tether } from "./fixtures";
import type { Tetherd } from "./tetherd";

const pixel = emulator("pixel");
const booted = emulator("pixel", { status: "Booted" });
const pixel8 = android("R58M", { name: "Pixel 8" });
const away = {
	...pixel8,
	condition: "Available" as const,
};

async function on(tetherd: Tetherd, tether: Tether, device: typeof pixel) {
	await tether.open(`${route(device.id)}/devices`);
	await connect(tetherd, { devices: [device] });
	await tether.ready();
}

/** Two clicks in the same moment, as a double click lands before the page redraws. */
async function twice(tether: Tether, name: string) {
	await tether.page.getByRole("button", { name }).evaluate((button: HTMLElement) => {
		button.click();
		button.click();
	});
}

test.describe("an emulator", () => {
	// While it shows booting or stopping, the page also watches the emulator's
	// screen for pictures, which these tests leave to screen.spec.ts.
	test.afterEach(({ tetherd }) => {
		ignoreTopic(tetherd, { Emulator: "pixel" });
		tetherd.ignore("Lane", { job: "Watch" });
	});

	test("shows booting the moment start is pressed, and starts once", async ({
		tetherd,
		tether,
	}) => {
		await on(tetherd, tether, pixel);

		await twice(tether, "Start pixel");

		const call = await tetherd.next("Call", { request: { Start: { name: "pixel" } } });

		await expect(tether.page.getByRole("button", { name: "pixel is busy" })).toBeDisabled();
		await expect(tether.card("pixel")).toContainText("Booting");

		call.reply(ok("pixel is starting"));
		tetherd.send({ Device: emulator("pixel", { status: "Booting" }) });
		await tetherd.next("Lane", { media: { Watch: { device: pixel.id } } });
		tetherd.send({ Device: booted });
		await expect(tether.page.getByRole("button", { name: "Stop pixel" })).toBeEnabled();
		tetherd.ignore("Offer");
		tetherd.ignore("Lane");
		tetherd.ignore("Adb");
	});

	test("shows stopping the moment stop is pressed, and stops once", async ({ tetherd, tether }) => {
		phone(tetherd, booted.id);
		noScreens(tetherd);
		await on(tetherd, tether, booted);

		await twice(tether, "Stop pixel");

		const call = await tetherd.next("Call", { request: { Stop: { name: "pixel" } } });

		await expect(tether.page.getByRole("button", { name: "pixel is busy" })).toBeDisabled();
		await expect(tether.card("pixel")).toContainText("Stopping");

		call.reply(ok("pixel is stopping"));
		tetherd.send({ Device: pixel });
		await expect(tether.page.getByRole("button", { name: "Start pixel" })).toBeEnabled();
	});

	test("a start tetherd refuses goes back to shut down, with why", async ({ tetherd, tether }) => {
		await on(tetherd, tether, pixel);

		await tether.page.getByRole("button", { name: "Start pixel" }).click();

		const call = await tetherd.next("Call", { request: { Start: {} } });

		await expect(tether.card("pixel")).toContainText("Booting");
		call.reply(err("emulator failed: no system image"));

		await expect(tether.toast()).toHaveText("emulator failed: no system image");
		await expect(tether.page.getByRole("button", { name: "Start pixel" })).toBeEnabled();
		await expect(tether.card("pixel")).toContainText("Shutdown");
	});

	test("a start tetherd says it began but never shows goes back, and says so", async ({
		tetherd,
		tether,
	}) => {
		// The page gives tetherd `PATIENCE` (link/pending.rs), which the test's
		// clock skips.
		await tether.page.clock.install();
		await on(tetherd, tether, pixel);

		await tether.page.getByRole("button", { name: "Start pixel" }).click();
		(await tetherd.next("Call", { request: { Start: {} } })).reply(ok("pixel is starting"));
		await expect(tether.toast()).toHaveText("pixel is starting");
		await expect(tether.card("pixel")).toContainText("Booting");

		await tether.page.clock.fastForward(10_000);

		await expect(tether.toast()).toHaveText("start failed: pixel did not start");
		await expect(tether.page.getByRole("button", { name: "Start pixel" })).toBeEnabled();
		await expect(tether.card("pixel")).toContainText("Shutdown");
		// The connection's own retries run on the same clock, and this test
		// does not answer them.
		tetherd.ignore("Hello");
	});

	test("tetherd saying it is still shut down wins over what the page showed", async ({
		tetherd,
		tether,
	}) => {
		await on(tetherd, tether, pixel);

		await tether.page.getByRole("button", { name: "Start pixel" }).click();

		const call = await tetherd.next("Call", { request: { Start: {} } });

		await expect(tether.card("pixel")).toContainText("Booting");
		tetherd.send({ Device: pixel });

		await expect(tether.page.getByRole("button", { name: "Start pixel" })).toBeEnabled();
		await expect(tether.card("pixel")).toContainText("Shutdown");
		call.reply(ok("pixel is starting"));
	});
});

test.describe("a phone", () => {
	test("shows disconnected the moment disconnect is pressed, and disconnects once", async ({
		tetherd,
		tether,
	}) => {
		phone(tetherd, pixel8.id);
		noScreens(tetherd);
		await on(tetherd, tether, pixel8);

		await twice(tether, "Disconnect Pixel 8");

		const call = await tetherd.next("Call", { request: { Disconnect: { device: pixel8.id } } });

		await expect(tether.page.getByRole("button", { name: "Connect to Pixel 8" })).toBeDisabled();
		await expect(tether.card("Pixel 8")).toContainText("Available");

		call.reply(ok("Disconnected from Pixel 8"));
		tetherd.send({ Device: away });
		await expect(tether.page.getByRole("button", { name: "Connect to Pixel 8" })).toBeEnabled();
	});

	test("a disconnect tetherd refuses shows it connected again, with why", async ({
		tetherd,
		tether,
	}) => {
		phone(tetherd, pixel8.id);
		noScreens(tetherd);
		await on(tetherd, tether, pixel8);

		await tether.page.getByRole("button", { name: "Disconnect Pixel 8" }).click();

		const call = await tetherd.next("Call", { request: { Disconnect: {} } });

		await expect(tether.card("Pixel 8")).toContainText("Available");
		call.reply(err("disconnect failed: in use"));

		await expect(tether.toast()).toHaveText("disconnect failed: in use");
		await expect(tether.page.getByRole("button", { name: "Disconnect Pixel 8" })).toBeEnabled();
		await expect(tether.card("Pixel 8")).toContainText("Connected");
	});

	test("shows connecting the moment connect is pressed, and connects once", async ({
		tetherd,
		tether,
	}) => {
		await on(tetherd, tether, away);

		await twice(tether, "Connect to Pixel 8");

		const call = await tetherd.next("Call", { request: { Connect: { device: pixel8.id } } });

		await expect(tether.card("Pixel 8")).toContainText("Connecting");
		await expect(tether.page.getByRole("button", { name: "Disconnect Pixel 8" })).toBeDisabled();

		call.reply(ok("Connecting to Pixel 8"));
		phone(tetherd, pixel8.id);
		noScreens(tetherd);
		tetherd.send({ Device: pixel8 });
		await expect(tether.page.getByRole("button", { name: "Disconnect Pixel 8" })).toBeEnabled();
		await expect(tether.card("Pixel 8")).toContainText("Connected");
	});
});
