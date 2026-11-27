// The imports run before `init`, so the globals they set exist when the
// WebAssembly starts.

import "#generated/main.css";
import "./capture";
import "./fonts";
import "./registerServiceWorker";
import "./terminal";

import init from "#generated/web.js";

await init();
