import { android, iphone, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, noScreens, phone } from "./helpers";

const ENTRIES: [title: string, key: string, heading: RegExp][] = [
	["Registry", "registry", /Registry/],
	["Apps", "apps", /Apps/],
	["Logs", "logs", /Logs/],
	["Crashes", "crashes", /Crashes/],
	["Shell", "shell", /Shell/],
	["Preferences", "preferences", /Preferences/],
	["Files", "files", /Files/],
	["Database", "database", /Database/],
	["Media", "media", /Media/],
	["Clipboard", "clipboard", /Clipboard/],
	["Camera", "camera", /Camera/],
	["Location", "location", /Location/],
	["Settings", "settings", /Settings/],
	["Access", "access", /Access/],
	["Network", "network", /Network/],
	["Events", "events", /Events/],
	["Devices", "devices", /Devices/],
];

test("every dock entry opens its view", async ({ tetherd, tether }) => {
	const pixel8 = android("R58M", { name: "Pixel 8" });

	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	await tether.open();
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	// This test is about switching views. The requests each view then makes of
	// tetherd and the phone are left to the views' own specs.
	for (const kind of ["Adb", "Lane", "Call", "Subscribe", "Query", "Unsubscribe"] as const) {
		tetherd.ignore(kind);
	}

	for (const [title, key, heading] of ENTRIES) {
		await tether.dock(title).click();

		await expect(tether.page, `${title} changes the address`).toHaveURL(new RegExp(`/${key}`));
		await expect(
			tether.page.getByRole("main").getByRole("heading", { level: 1 }).first(),
			`${title} shows its view`,
		).toHaveText(heading);
	}
});

test("an iPhone has no Apps view yet", async ({ tetherd, tether }) => {
	const troy = iphone("00008130", { name: "TroyiPhone" });

	noScreens(tetherd);
	// The tunnel an iPhone's input goes through, which this test leaves alone.
	tetherd.ignore("Lane", { job: "Ios" });
	await tether.open(`${route(troy.id)}/apps`);
	await connect(tetherd, { devices: [troy] });
	await tether.ready();

	await expect(tether.page.getByText("TroyiPhone has no Apps.")).toBeVisible();
});
