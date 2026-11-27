function start(module: WebAssembly.Module): Worker {
	// rspack bundles the worker on its own when it sees this exact expression.
	const started = new Worker(new URL("./worker/index.ts", import.meta.url), { type: "module" });
	started.postMessage(module);

	return started;
}

Object.assign(globalThis, { tetherCapture: { start } });
