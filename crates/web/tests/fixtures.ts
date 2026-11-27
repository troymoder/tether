import { test as base, expect, type Locator, type Page } from "@playwright/test";
import { ORIGIN, Tetherd, describe, endWaits } from "./tetherd";

export class Tether {
	constructor(readonly page: Page) {}

	async open(address = "") {
		await this.page.goto(`${ORIGIN}/#${address}`);
	}

	async at(): Promise<string> {
		return decodeURIComponent(new URL(this.page.url()).hash.slice(1));
	}

	async ready() {
		await expect(this.page.locator("main")).toHaveAttribute("aria-busy", "false");
	}

	dock(title: string): Locator {
		return this.page.getByRole("navigation", { name: "Views" }).getByTitle(title, { exact: true });
	}

	toast(): Locator {
		// The device pane's "Reconnecting" cover is a status too.
		return this.page.locator("[role=status][popover]");
	}

	card(name: string): Locator {
		return this.page
			.locator("[data-card]")
			.filter({ has: this.page.locator("strong").getByText(name, { exact: true }) });
	}

	async reload() {
		await this.page.reload();
	}
}

export const test = base.extend<{ tetherd: Tetherd; tether: Tether }>({
	// It depends on `page` so the page is made first. With a reused context, as
	// the editor's "Show browser" runs, `page` is the context's first page, which
	// would otherwise be the peer's.
	tetherd: async ({ context, page: _ }, use) => {
		const tetherd = await Tetherd.start(context);

		await use(tetherd);
		endWaits();

		// Everything the page asked for must be answered, or the test marks it
		// with `ignore`.
		expect(tetherd.left().map(describe), "events the test left unanswered").toEqual([]);
	},
	// It depends on `tetherd` so the routes are in place before the page loads.
	tether: async ({ page, tetherd: _ }, use) => {
		// A Rust panic reaches the console through console_error_panic_hook, and
		// a JavaScript error is a `pageerror`.
		const panics: string[] = [];

		page.on("pageerror", (error) => panics.push(error.message));
		page.on("console", (message) => {
			if (message.type() === "error" && message.text().includes("panicked")) {
				panics.push(message.text());
			}
		});

		// The peer's tab opens after this page. In a headed browser that hides
		// this one, and Chrome then stops its animation frames and slows its timers.
		await page.bringToFront();
		await use(new Tether(page));
		expect(panics, "the page panicked").toEqual([]);
	},
});

export { expect };
