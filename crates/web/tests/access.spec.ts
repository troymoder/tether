import type { Client } from "./protocol";
import { YOU, access } from "./builders";
import { type Tether, expect, test } from "./fixtures";
import { connect, ok } from "./helpers";

function client(name: string): Client {
	return { print: `SHA256:${name}`, name, kind: "Browser", at: 0 };
}

const me = { print: YOU, name: "Chromium on Linux", kind: "Browser", at: 0 } as const;

test.beforeEach(({ tetherd }) => {
	// The primary's emulator options under Instances, and this browser's USB
	// report, are outside these tests.
	tetherd.ignore("Lane", { job: "EmulatorOptions" });
	tetherd.ignore("Usb");
});

function approve(tether: Tether, name: string) {
	return tether.page
		.locator("div")
		.filter({ hasText: name })
		.filter({ has: tether.page.getByRole("button", { name: "Approve" }) })
		.last()
		.getByRole("button", { name: "Approve" });
}

async function approvals(tether: Tether) {
	await tether.page.getByRole("tab", { name: /Approvals/ }).click();
}

test("approve trusts the client its row shows after another is rejected", async ({
	tetherd,
	tether,
}) => {
	const [first, second] = [client("First"), client("Second")];
	const reject = tether.page.getByRole("button", { name: "Reject" }).first();

	await tether.open("/access");
	await connect(tetherd, { access: access({ asking: [first, second] }) });
	await tether.ready();
	await approvals(tether);

	await reject.click();
	(await tetherd.next("Call", { request: { Reject: { print: first.print } } })).reply(
		ok("Rejected First"),
	);
	tetherd.send({ Value: { Access: access({ asking: [second] }) } });
	await expect(tether.page.getByText("First", { exact: true })).toHaveCount(0);

	await approve(tether, "Second").click();

	const call = await tetherd.next("Call", { request: { Approve: {} } });

	expect(call.request).toEqual({ Approve: { print: second.print } });
	call.reply(ok("Approved Second"));
	await tether.page.waitForTimeout(300);
	expect(tetherd.left().filter((event) => event.kind === "Call")).toEqual([]);
});

test("approve trusts the client its row shows after tetherd drops the oldest asking", async ({
	tetherd,
	tether,
}) => {
	const [oldest, middle, newest] = [client("Oldest"), client("Middle"), client("Newest")];

	await tether.open("/access");
	await connect(tetherd, { access: access({ asking: [oldest, middle] }) });
	await tether.ready();
	await approvals(tether);
	await expect(tether.page.getByText("Middle", { exact: true })).toBeVisible();

	// tetherd keeps a few asking clients and drops the oldest for a new one.
	tetherd.send({ Value: { Access: access({ asking: [middle, newest] }) } });
	await expect(tether.page.getByText("Newest", { exact: true })).toBeVisible();

	// A row that showed Oldest or Middle before may show Newest now.
	await approve(tether, "Newest").click();

	const call = await tetherd.next("Call", { request: { Approve: {} } });

	expect(call.request, "the client the row shows").toEqual({ Approve: { print: newest.print } });
	call.reply(ok("Approved Newest"));
	await tether.page.waitForTimeout(300);
	expect(tetherd.left().filter((event) => event.kind === "Call")).toEqual([]);
});

test("revoke names the client its row shows after another went", async ({ tetherd, tether }) => {
	const [first, second] = [client("First"), client("Second")];

	await tether.open("/access");
	await connect(tetherd, { access: access({ trusted: [me, first, second] }) });
	await tether.ready();

	tetherd.send({ Value: { Access: access({ trusted: [me, second] }) } });
	await expect(tether.page.getByText("First", { exact: true })).toHaveCount(0);

	await tether.page.getByRole("button", { name: "Revoke" }).nth(1).click();
	await expect(tether.page.getByRole("alertdialog")).toContainText("Revoke Second?");
	await tether.page.getByRole("alertdialog").getByRole("button", { name: "Revoke" }).click();

	const call = await tetherd.next("Call", { request: { Revoke: {} } });

	expect(call.request).toEqual({ Revoke: { print: second.print } });
	call.reply(ok("Revoked Second"));
	await tether.page.waitForTimeout(300);
	expect(tetherd.left().filter((event) => event.kind === "Call")).toEqual([]);
});

test("unlink names the instance its row shows after another went", async ({ tetherd, tether }) => {
	const primary = { id: "SHA256:primary", name: "test" };
	const [first, second] = [
		{ id: "SHA256:first", name: "First box" },
		{ id: "SHA256:second", name: "Second box" },
	];

	await tether.open("/access");
	await connect(tetherd, { access: access({ instances: [primary, first, second] }) });
	await tether.ready();

	tetherd.send({ Value: { Access: access({ instances: [primary, second] }) } });
	await expect(tether.page.getByText("First box", { exact: true })).toHaveCount(0);

	await tether.page.getByRole("button", { name: "Unlink" }).click();
	await expect(tether.page.getByRole("alertdialog")).toContainText("Unlink Second box?");
});

test("forget takes back the phone its row shows after another was forgotten", async ({
	tetherd,
	tether,
}) => {
	// Stubs the browser's allowed phones, because WebUSB has none in a test.
	await tether.page.addInitScript(() => {
		const forgot: string[] = [];
		const phone = (name: string) => ({
			productName: name,
			serialNumber: name,
			opened: false,
			forget: async () => {
				forgot.push(name);
				phones.splice(phones.indexOf(phone_of[name]), 1);
			},
		});
		const phone_of: Record<string, ReturnType<typeof phone>> = {};
		const phones = ["Alpha", "Beta"].map((name) => (phone_of[name] = phone(name)));

		Object.assign(window, { forgot });
		// The DOM types the tests build with have no WebUSB.
		const usb = (
			globalThis as unknown as { USB: { prototype: { getDevices(): Promise<unknown[]> } } }
		).USB;

		usb.prototype.getDevices = async () => [...phones];
	});
	await tether.open("/settings");
	await connect(tetherd);
	await tether.ready();

	const forget = tether.page.getByRole("button", { name: "Forget" });

	await expect(forget).toHaveCount(2);
	await forget.first().click();
	await expect(forget).toHaveCount(1);
	await expect(tether.page.getByText("Alpha", { exact: true })).toHaveCount(0);

	await forget.click();
	await expect(forget).toHaveCount(0);
	expect(
		await tether.page.evaluate(() => (window as unknown as { forgot: string[] }).forgot),
	).toEqual(["Alpha", "Beta"]);
});
