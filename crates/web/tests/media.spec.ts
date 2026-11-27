import { android, route } from "./builders";
import { expect, test } from "./fixtures";
import { connect, noScreens, phone } from "./helpers";
import { type Helper, Kind, helper } from "./helper";
import type { Tetherd } from "./tetherd";
import type { Tether } from "./fixtures";

const pixel8 = android("R58M", { name: "Pixel 8" });
const CAMERA = "/storage/emulated/0/DCIM/Camera";

async function gallery(tetherd: Tetherd, tether: Tether): Promise<Helper> {
	const played = helper(tetherd, phone(tetherd, pixel8.id));

	played.add({ path: `${CAMERA}/old.jpg`, changed: 100 });
	played.add({ path: `${CAMERA}/mid.jpg`, changed: 200 });
	played.add({ path: `${CAMERA}/new.jpg`, changed: 300 });
	noScreens(tetherd);
	await tether.open(`${route(pixel8.id)}/media`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	return played;
}

test("the gallery shows the phone's photos, newest first", async ({ tetherd, tether }) => {
	const played = await gallery(tetherd, tether);
	const opens = tether.page.getByRole("button", { name: /^Open .* at full size$/ });

	await expect(opens).toHaveText([/^new\.jpg/, /^mid\.jpg/, /^old\.jpg/]);
	expect(played.asked.find((asked) => asked.kind === Kind.Count)?.arguments).toContain(CAMERA);
});

test("pick in the viewer picks the photo it shows after stepping", async ({ tetherd, tether }) => {
	await gallery(tetherd, tether);
	await tether.page.getByRole("button", { name: "Open new.jpg at full size" }).click();
	await expect(tether.page.getByRole("dialog", { name: "new.jpg, at full size" })).toBeVisible();

	await tether.page.keyboard.press("ArrowRight");
	await expect(tether.page.getByRole("dialog", { name: "mid.jpg, at full size" })).toBeVisible();
	await tether.page.getByRole("button", { name: "Pick", exact: true }).click();

	await expect(tether.page.getByRole("button", { name: "Picked", exact: true })).toBeVisible();
	await tether.page.getByRole("button", { name: "Back to all the media" }).click();
	await expect(tether.page.getByRole("checkbox", { name: "Pick mid.jpg" })).toBeChecked();
	await expect(tether.page.getByRole("checkbox", { name: "Pick new.jpg" })).not.toBeChecked();
});
