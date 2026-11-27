// The worker starts from the module the page already compiled, so nothing is
// fetched or compiled twice.

import init, { captureWorker } from "#generated/web.js";

self.onmessage = async ({ data }: MessageEvent<WebAssembly.Module>) => {
	// Later messages go to the listener that `captureWorker` installs.
	self.onmessage = null;
	await init({ module_or_path: data });
	captureWorker();
};
