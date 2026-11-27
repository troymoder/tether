import { android, line, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, noScreens, phone } from "./helpers";

test("the page moves on when the picked device goes", async ({ tetherd, tether }) => {
	const first = android("R58M", { name: "Pixel 8" });
	const second = android("ZY22", { name: "Moto G" });

	phone(tetherd, first.id);
	phone(tetherd, second.id);
	noScreens(tetherd);
	await tether.open(`${route(first.id)}/devices`);
	await connect(tetherd, { devices: [first, second] });
	await tether.ready();
	await expect.poll(() => tether.at()).toBe(`${line(first.id)}/devices`);

	tetherd.send({ DeviceGone: first.id });
	await expect.poll(() => tether.at()).toBe(`${line(second.id)}/devices`);

	tetherd.send({ DeviceGone: second.id });
	await expect(tether.page.getByRole("button", { name: "No device" })).toBeDisabled();
});
