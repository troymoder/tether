import { android, route } from "./builders";
import { expect, test } from "./fixtures";
import { apps, connect, noScreens, ok, phone } from "./helpers";
import { Kind, helper } from "./helper";

const pixel8 = android("R58M", { name: "Pixel 8" });
const APK = "/data/app/com.example-1/base.apk";

test("an installed app added to the registry hands over the APK it pulled", async ({
	tetherd,
	tether,
}) => {
	const device = phone(tetherd, pixel8.id, [
		[/^pm list packages/, `package:${APK}=com.example versionCode:3\n`],
		[/^pm path /, `package:${APK}\n`],
	]);
	const apk = new Uint8Array(200 * 1024).map((_, at) => at % 251);

	// Bigger than one sync chunk, so the pull comes in several.
	device.files.set(APK, apk);
	noScreens(tetherd);
	// The app in front, and the helper that reads labels, are not in this test.
	tetherd.ignore("Adb", (event) => event.service.includes("dumpsys activity activities"));
	tetherd.ignore("Lane", { job: "AndroidHelper" });
	await tether.open(`${route(pixel8.id)}/apps`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();
	await apps(tetherd, []);

	await tether.page.getByRole("button", { name: /^More for / }).click();
	await tether.page.getByText("Add to registry").click();

	// An APK is too big for the control channel, so the call comes on a lane.
	const call = await tetherd.next("Lane", {
		media: { Call: { request: { HandOver: { id: "com.example", from: pixel8.id } } } },
	});
	const handed = (call.media as { Call: { request: { HandOver: { apk: number[] } } } }).Call.request
		.HandOver.apk;

	expect(new Uint8Array(handed)).toEqual(apk);
	call.reply(ok("Added com.example"));
	await expect(tether.toast()).toHaveText("Added com.example");
});

test("an APK the device can't give says why", async ({ tetherd, tether }) => {
	// No file at APK, so the pull fails as adbd fails it.
	phone(tetherd, pixel8.id, [
		[/^pm list packages/, `package:${APK}=com.example versionCode:3\n`],
		[/^pm path /, `package:${APK}\n`],
	]);
	noScreens(tetherd);
	tetherd.ignore("Adb", (event) => event.service.includes("dumpsys activity activities"));
	tetherd.ignore("Lane", { job: "AndroidHelper" });
	await tether.open(`${route(pixel8.id)}/apps`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();
	await apps(tetherd, []);

	await tether.page.getByRole("button", { name: /^More for / }).click();
	await tether.page.getByText("Add to registry").click();

	await expect(tether.toast()).toHaveText(
		"pull failed: adb failed: sync: device error: open failed: No such file or directory",
	);
	await tetherd.next("Event", { what: "apps", failed: true, about: pixel8.id });
});

test("an app's row shows the label the phone gives it", async ({ tetherd, tether }) => {
	const device = phone(tetherd, pixel8.id, [
		[/^pm list packages/, `package:${APK}=com.example versionCode:3\n`],
	]);
	const played = helper(tetherd, device);

	played.apps.set("com.example", { label: "Example", version: "2.4.0" });
	noScreens(tetherd);
	tetherd.ignore("Adb", (event) => event.service.includes("dumpsys activity activities"));
	await tether.open(`${route(pixel8.id)}/apps`);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();
	await apps(tetherd, []);

	await expect(tether.page.locator("strong").getByText("Example", { exact: true })).toBeVisible();
	expect(played.asked).toContainEqual({ kind: Kind.Apps, arguments: ["com.example"] });
});
