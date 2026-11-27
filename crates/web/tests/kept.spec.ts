import { stored } from "./builders";
import { expect, test } from "./fixtures";
import { connect } from "./helpers";

/** Runs in the page. */
function preference(key: string): Promise<unknown> {
	return new Promise((done, fail) => {
		const opening = indexedDB.open("tether");

		opening.onerror = () => fail(opening.error);
		opening.onsuccess = () => {
			const reading = opening.result.transaction("preferences").objectStore("preferences").get(key);

			reading.onerror = () => fail(reading.error);
			reading.onsuccess = () => {
				opening.result.close();
				done(reading.result);
			};
		};
	});
}

test("the theme is kept across a reload", async ({ tetherd, tether }) => {
	await tether.open();
	await connect(tetherd);
	await tether.ready();

	await tether.page.getByRole("radiogroup", { name: "Theme" }).getByText("Dark").click();
	await expect(tether.page.locator("html")).toHaveAttribute("data-theme", "dark");
	// The page writes it in the background (app/storage.rs), and a reload
	// before the write commits loses it.
	await expect.poll(() => tether.page.evaluate(preference, "theme")).toBe("dark");

	await tether.reload();
	await connect(tetherd);
	await tether.ready();
	await expect(tether.page.locator("html")).toHaveAttribute("data-theme", "dark");
});

// The editor runs every test in one browser (`playwright.reuseBrowser`), and
// Playwright clears its storage between them. A theme the test above kept
// would show here otherwise.
test("a test starts with nothing stored", async ({ tetherd, tether }) => {
	await tether.open();
	await connect(tetherd);
	await tether.ready();

	expect(await tether.page.evaluate(preference, "theme")).toBeUndefined();
});

test.describe("with a dark theme kept from before", () => {
	test.use({ storageState: stored({ theme: "dark" }) });

	test("the page opens dark", async ({ tetherd, tether }) => {
		await tether.open();
		await connect(tetherd);
		await tether.ready();

		await expect(tether.page.locator("html")).toHaveAttribute("data-theme", "dark");
	});
});
