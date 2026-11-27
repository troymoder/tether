// Each code has a `Media::Pair` lane of its own, on which tetherd waits for
// the phone that scans it and reports each step. A code lasts two minutes,
// renews itself for ten while the page is in view, and closing its lane ends
// tetherd's wait.

import { android, line } from "./builders";
import { expect, test } from "./fixtures";
import { connect, noScreens, phone } from "./helpers";
import { encodePairing } from "./protocol";
import type { Pairing } from "./protocol";
import type { Tether } from "./fixtures";
import type { EventOf, Tetherd } from "./tetherd";

const pixel8 = android("R58M", { name: "Pixel 8" });

async function pairing(tetherd: Tetherd, tether: Tether): Promise<EventOf<"Lane">> {
	// The box opens on creating an emulator, whose options it asks tetherd for.
	tetherd.ignore("Lane", { job: "EmulatorOptions" });
	await tether.page.getByRole("button", { name: "Add device" }).click();
	await tether.page.getByRole("tab", { name: "Physical" }).click();
	await tether.page.getByRole("button", { name: "Pair a new phone" }).click();
	await expect(
		tether.page.getByRole("img", { name: "QR code for the phone to scan" }),
	).toBeVisible();

	return tetherd.next("Lane", { job: "Pair" });
}

function code(lane: EventOf<"Lane">): { name: string; password: string } {
	return (lane.media as { Pair: { name: string; password: string } }).Pair;
}

function tell(lane: EventOf<"Lane">, step: Pairing) {
	lane.channel.send(encodePairing(step));
}

function card(tether: Tether) {
	return tether.page.locator("[class*=code-card]");
}

function foot(tether: Tether) {
	return tether.page.locator("[class*=code-foot]");
}

test("a phone that scans the code is paired, connected and picked", async ({ tetherd, tether }) => {
	phone(tetherd, pixel8.id);
	noScreens(tetherd);
	await tether.open("/devices");
	await connect(tetherd, { devices: [pixel8] });
	await tether.ready();

	const lane = await pairing(tetherd, tether);

	// The name and password Android's Wireless debugging reads from the code.
	expect(code(lane).name).toMatch(/^tether-[a-z2-9]{6}$/);
	expect(code(lane).password).toMatch(/^[a-z2-9]{10}$/);
	await expect(foot(tether)).toHaveText(/^Expires in [12]:\d\d$/);

	tell(lane, { Scanned: { address: "192.168.1.24:37529" } });
	await expect(card(tether).getByText("192.168.1.24:37529")).toBeVisible();
	await expect(card(tether).getByText("Pairing…")).toBeVisible();
	await expect(foot(tether)).toHaveText("Scanned");

	tell(lane, "Connecting");
	await expect(card(tether).getByText("Connecting…")).toBeVisible();

	tell(lane, { Connected: { device: pixel8.id } });
	lane.channel.end();
	await expect(tether.page.getByRole("heading", { name: "Pair an Android phone" })).toHaveCount(0);
	await expect.poll(() => tether.at()).toMatch(new RegExp(`^${line(pixel8.id)}`));
});

test("a code renews itself, then expires and is made again by hand", async ({
	tetherd,
	tether,
}) => {
	// Two minutes per code and ten of renewing, which the test's clock skips.
	await tether.page.clock.install();
	await tether.open("/devices");
	await connect(tetherd);
	await tether.ready();
	// The connection's own retries run on the same clock, and this test does
	// not answer them.
	tetherd.ignore("Hello");

	const first = await pairing(tetherd, tether);

	await tether.page.clock.fastForward(120_000);

	// A code that runs out while the page is in view is replaced, and the
	// first code's lane is closed, which ends tetherd's wait for it.
	const second = await tetherd.next("Lane", { job: "Pair" });

	expect(code(second).name).not.toBe(code(first).name);
	await first.channel.gone;
	await expect(foot(tether)).toHaveText("New code");

	// Ten minutes after the code asked for by hand, the next one runs out and
	// is not renewed.
	for (let renewed = 0; renewed < 3; renewed++) {
		await tether.page.clock.fastForward(120_000);
		await tetherd.next("Lane", { job: "Pair" });
	}

	await tether.page.clock.fastForward(120_000);
	await expect(foot(tether)).toHaveText("Expired");

	await tether.page.getByRole("button", { name: "The code expired. Make a new code" }).click();
	await tetherd.next("Lane", { job: "Pair" });
	await expect(foot(tether)).toHaveText(/^Expires in [12]:\d\d$/);
	// The lanes of the replaced codes closed and are not answered.
	tetherd.ignore("Lane", { job: "Pair" });
});

test("a pairing that fails says why, and the code is expired", async ({ tetherd, tether }) => {
	await tether.open("/devices");
	await connect(tetherd);
	await tether.ready();

	const lane = await pairing(tetherd, tether);

	tell(lane, { Scanned: { address: "192.168.1.24:37529" } });
	tell(lane, { Failed: "adb failed: pairing failed" });
	lane.channel.end();

	await expect(tether.toast()).toHaveText("adb failed: pairing failed");
	await expect(foot(tether)).toHaveText("Expired");
	await expect(
		tether.page.getByRole("button", { name: "The code expired. Make a new code" }),
	).toBeVisible();
});
