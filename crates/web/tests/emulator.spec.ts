import { emulator, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, ignoreTopic, ok } from "./helpers";

test("the start button starts the emulator, and turns to stop once it boots", async ({
	tetherd,
	tether,
}) => {
	const pixel = emulator("pixel");

	await tether.open(`${route(pixel.id)}/devices`);
	await connect(tetherd, { devices: [pixel] });
	await tether.ready();

	await tether.page.getByRole("button", { name: "Start pixel" }).click();

	const call = await tetherd.next("Call", { request: { Start: { name: "pixel" } } });

	call.reply(ok("pixel is starting"));
	await expect(tether.toast()).toHaveText("pixel is starting");

	tetherd.send({ Device: emulator("pixel", { status: "Booting" }) });
	await expect(tether.page.getByRole("button", { name: "pixel is busy" })).toBeDisabled();

	const watch = await tetherd.next("Lane", { media: { Watch: { device: pixel.id } } });

	tetherd.send({ Device: emulator("pixel", { status: "Booted" }) });
	await expect(tether.page.getByRole("button", { name: "Stop pixel" })).toBeVisible();
	await watch.channel.gone;

	ignoreTopic(tetherd, { Emulator: "pixel" });
});

test("a failed start shows its error", async ({ tetherd, tether }) => {
	const pixel = emulator("pixel");

	await tether.open(`${route(pixel.id)}/devices`);
	await connect(tetherd, { devices: [pixel] });
	await tether.ready();

	await tether.page.getByRole("button", { name: "Start pixel" }).click();
	(await tetherd.next("Call", { request: { Start: {} } })).reply({
		Err: "emulator failed: no system image",
	});

	await expect(tether.toast()).toHaveText("emulator failed: no system image");
	await expect(tether.page.getByRole("button", { name: "Start pixel" })).toBeVisible();

	ignoreTopic(tetherd, { Emulator: "pixel" });
	// While it showed the emulator booting, the page watched for its pictures.
	tetherd.ignore("Lane", { job: "Watch" });
});
