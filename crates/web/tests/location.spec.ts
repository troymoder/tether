import { android, place, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, noScreens, ok, phone, topic } from "./helpers";

test("forgetting a kept place forgets that place alone", async ({ tetherd, tether }) => {
	const pixel8 = android("R58M", { name: "Pixel 8" });
	const row = (name: string) =>
		tether.page.getByTitle("Put the device here").filter({ hasText: name }).locator("..");

	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	// The view also follows the device's position, which this test leaves alone.
	tetherd.ignore("Adb");
	tetherd.ignore("Lane");
	await tether.open(`${route(pixel8.id)}/location`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();
	await topic(tetherd, "Places", { Places: [place("Home"), place("Work"), place("Gym")] });

	await row("Home").getByRole("button", { name: "Forget this place" }).click();
	(await tetherd.next("Call", { request: { ForgetPlace: { name: "Home" } } })).reply(
		ok("Forgot Home"),
	);
	tetherd.send({ Value: { Places: [place("Work"), place("Gym")] } });
	await expect(row("Home")).toHaveCount(0);

	// Gym's row was Work's before Home went.
	await row("Gym").getByRole("button", { name: "Forget this place" }).click();
	const forgot = await tetherd.next("Call", { request: { ForgetPlace: {} } });

	expect(forgot.request).toEqual({ ForgetPlace: { name: "Gym" } });
	forgot.reply(ok("Forgot Gym"));
	// A handler left from the earlier list would ask more in the same click.
	await tether.page.waitForTimeout(300);
	expect(tetherd.left().filter((event) => event.kind === "Call")).toEqual([]);
});
