// Run `bunx playwright test` in crates/web while `just dev up playwright`
// runs. That process serves the page, and `test-codec-build` keeps the codec
// in tests/dist/codec.
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
	testDir: "tests",
	outputDir: "../../target/playwright",
	fullyParallel: true,
	reporter: [["list"]],
	// Everything is local, so what the page does shows at once. The test.fail
	// tests wait this long before they fail as expected.
	expect: { timeout: 5000 },
	use: {
		...devices["Desktop Chrome"],
		// The tests answer the page's signalling with routes, which miss requests
		// the page's Service Worker (glue/serviceWorker/index.ts) handles.
		serviceWorkers: "block",
		trace: "retain-on-failure",
	},
	projects: [{ name: "chromium" }],
});
