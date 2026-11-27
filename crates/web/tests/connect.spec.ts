import { android, emulator, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, ignoreTopic, noScreens, phone } from "./helpers";

test("shows the devices tetherd lists", async ({ tetherd, tether }) => {
	const pixel8 = android("R58M", { name: "Pixel 8" });

	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	await tether.open();
	await connect(tetherd, { devices: [pixel8, emulator("pixel")] });
	await tether.ready();

	await expect(tether.page.getByText("Pixel 8").first()).toBeVisible();
	await expect(tether.page.getByText("pixel", { exact: true }).first()).toBeVisible();

	// The emulator's row follows its settings, which this test doesn't look at.
	ignoreTopic(tetherd, { Emulator: "pixel" });
});

test("a device tetherd adds shows, and one it drops goes", async ({ tetherd, tether }) => {
	const pixel8 = android("R58M", { name: "Pixel 8" });

	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	await tether.open(`${route(pixel8.id)}/devices`);
	await connect(tetherd);
	await tether.ready();

	tetherd.send({ Device: pixel8 });
	await expect(tether.page.getByText("Pixel 8").first()).toBeVisible();

	tetherd.send({ DeviceGone: pixel8.id });
	await expect(tether.page.getByText("Pixel 8")).toHaveCount(0);
});

test("a gate shows the setup screen", async ({ tetherd, tether }) => {
	await tether.open();
	await (await tetherd.next("Hello")).reply();
	await (await tetherd.next("Connect")).gate({
		Setup: { primary: "SHA256:primary", you: "SHA256:test", missed: false },
	});

	await expect(tether.page.getByText(/setup code/i).first()).toBeVisible();

	// The page asks again every 2 s while it waits at the gate.
	tetherd.ignore("Hello");
});
