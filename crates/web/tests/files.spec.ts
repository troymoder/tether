import { android, route } from "./builders";
import { expect, test } from "./fixtures";
import { type Script, apps, connect, listing, noScreens, phone } from "./helpers";

const pixel8 = android("R58M", { name: "Pixel 8" });

const FOLDERS: Script = [
	[
		"ls -lA /sdcard/",
		listing([
			{ name: "DCIM", folder: true },
			{ name: "notes.txt", size: 12 },
		]),
	],
	["ls -lA /sdcard/DCIM/", listing([{ name: "Camera", folder: true }])],
];

test.beforeEach(async ({ tetherd, tether }) => {
	noScreens(tetherd);
	await tether.open(`${route(pixel8.id)}/files`);
});

test("opens a folder", async ({ tetherd, tether }) => {
	const device = phone(tetherd, pixel8.id, [...FOLDERS]);
	const entry = (name: string) =>
		tether.page.getByTitle("Double-click to open").filter({ hasText: name });

	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();
	await apps(tetherd, []);

	await expect(entry("notes.txt")).toBeVisible();
	await entry("DCIM").dblclick();
	await expect(tether.page).toHaveURL(/\/files\/sdcard\/DCIM$/);
	await expect(entry("Camera")).toBeVisible();
	expect(device.ran.filter((run) => run.startsWith("ls "))).toEqual([
		"ls -lA /sdcard/",
		"ls -lA /sdcard/DCIM/",
	]);
});

test("after a delete, each row's tick picks its own entry", async ({ tetherd, tether }) => {
	const all = [{ name: "a.txt" }, { name: "b.txt" }, { name: "c.txt" }];
	let listed = all;

	phone(tetherd, pixel8.id, [
		["ls -lA /sdcard/", () => listing(listed)],
		[
			/^rm -rf /,
			(run) => {
				listed = listed.filter(({ name }) => !run.includes(name));
				return "";
			},
		],
	]);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();
	await apps(tetherd, []);

	const tick = (name: string) => tether.page.getByRole("checkbox", { name: `Pick ${name}` });
	const row = (name: string) => tether.page.locator("[data-pick]").filter({ hasText: name });

	await row("a.txt").getByRole("button", { name: "Delete from the device" }).click();
	await tether.page.getByRole("button", { name: "Delete", exact: true }).click();
	await tetherd.next("Event", { what: "files", message: "Deleted 1 item", about: pixel8.id });
	await expect(row("a.txt")).toHaveCount(0);

	// c.txt now sits in the row b.txt had.
	await tick("c.txt").click();
	await expect(tick("c.txt")).toBeChecked();
	await expect(tick("b.txt")).not.toBeChecked();

	await row("c.txt").getByRole("button", { name: "Delete from the device" }).click();
	await expect(tether.page.getByRole("alertdialog", { name: "Delete c.txt?" })).toBeVisible();
});

test("a folder that can't be listed says why", async ({ tetherd, tether }) => {
	phone(tetherd, pixel8.id, [
		["ls -lA /sdcard/", { stderr: "ls: /sdcard/: Permission denied", exit: 1 }],
	]);
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();
	await apps(tetherd, []);

	await expect(tether.page.getByText(/Permission denied/).first()).toBeVisible();
});

test("an uploaded file is written into the open folder", async ({ tetherd, tether }) => {
	const written = new Map<string, string>();
	const device = phone(tetherd, pixel8.id, [...FOLDERS]);

	// `cat > <path>` takes the file on stdin, as a real shell would.
	device.on(/^cat > /, async (command, process) => {
		written.set(command.slice("cat > ".length), new TextDecoder().decode(await process.all()));

		return "";
	});
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();
	await apps(tetherd, []);
	await expect(tether.page.getByText("notes.txt")).toBeVisible();

	await tether.page.getByLabel("Files for this folder").setInputFiles({
		name: "hello.txt",
		mimeType: "text/plain",
		buffer: Buffer.from("hi from the test\n"),
	});

	await expect(tether.toast()).toHaveText("Saved /sdcard/hello.txt");
	// The page did the write by itself, so it tells tetherd for the events.
	await tetherd.next("Event", {
		what: "files",
		failed: false,
		message: "Saved /sdcard/hello.txt",
		about: pixel8.id,
	});
	expect(written).toEqual(new Map([["/sdcard/hello.txt", "hi from the test\n"]]));
});

test("an upload that ends after another device is picked is filed under its own", async ({
	tetherd,
	tether,
}) => {
	const motoG = android("ZY22", { name: "Moto G" });
	const device = phone(tetherd, pixel8.id, [...FOLDERS]);
	let finish!: () => void;
	const finished = new Promise<void>((done) => {
		finish = done;
	});

	phone(tetherd, motoG.id, [...FOLDERS]);
	device.on(/^cat > /, async (_, process) => {
		await process.all();
		await finished;

		return "";
	});
	await connect(tetherd, { devices: [pixel8, motoG] });
	await tether.ready();
	await apps(tetherd, []);
	await expect(tether.page.getByText("notes.txt")).toBeVisible();

	await tether.page.getByLabel("Files for this folder").setInputFiles({
		name: "hello.txt",
		mimeType: "text/plain",
		buffer: Buffer.from("hi"),
	});
	await expect.poll(() => device.ran.some((run) => run.startsWith("cat > "))).toBe(true);
	await tether.page.getByRole("button", { name: /^Device:/ }).click();
	await tether.page.getByRole("option", { name: /Moto G/ }).click();
	await expect(tether.page.getByRole("button", { name: /^Device: Moto G/ })).toBeVisible();
	finish();

	await tetherd.next("Event", {
		what: "files",
		message: "Saved /sdcard/hello.txt",
		about: pixel8.id,
	});
	// Moto G's Files view asks for the app registry when it opens.
	tetherd.ignore("Subscribe", { topic: "Apps" });
	tetherd.ignore("Query", { topic: "Apps" });
	tetherd.ignore("Unsubscribe", { topic: "Apps" });
});
