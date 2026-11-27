import { FitAddon } from "@xterm/addon-fit";
import { LigaturesAddon } from "@xterm/addon-ligatures";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { type ITerminalOptions, type ITheme, Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { loaded } from "./fonts";

const FAMILY = '"JetBrains Mono", ui-monospace';

const token = (name: string) =>
	getComputedStyle(document.documentElement).getPropertyValue(name).trim();

function build(options: ITerminalOptions) {
	const host = Object.assign(document.createElement("div"), { className: "terminal" });
	const terminal = new Terminal({
		fontFamily: FAMILY,
		fontSize: 12,
		// SearchAddon needs the proposed API to decorate matches.
		allowProposedApi: true,
		...options,
	});
	const fit = new FitAddon();

	terminal.loadAddon(fit);
	terminal.loadAddon(new WebLinksAddon());
	terminal.open(host);
	// The ligatures addon loads before the WebGL renderer, which has to know about it.
	terminal.loadAddon(new LigaturesAddon());

	try {
		const webgl = new WebglAddon();

		// On context loss xterm falls back to its DOM renderer.
		webgl.onContextLoss(() => webgl.dispose());
		terminal.loadAddon(webgl);
	} catch {
		// Without WebGL2 the DOM renderer draws it.
	}

	// The terminal measured its cells when it was built, possibly with a
	// fallback font. Setting the family again makes it measure once the font loads.
	void loaded.then(() => {
		terminal.options.fontFamily = `${FAMILY}, monospace`;

		if (host.isConnected && host.clientWidth > 0) {
			fit.fit();
		}
	});

	return {
		terminal,
		common: {
			host,
			// A detached or hidden element has no size to fit to.
			fit: () => {
				if (host.isConnected && host.clientWidth > 0) {
					fit.fit();
				}
			},
			focus: () => terminal.focus(),
			columns: () => terminal.cols,
			rows: () => terminal.rows,
			rowHeight: () => (host.querySelector(".xterm-screen")?.clientHeight ?? 0) / terminal.rows,
			colour: (theme: ITheme) => {
				terminal.options.theme = theme;
			},
		},
	};
}

function shell(
	typed: (sent: string) => void,
	sized: (columns: number, rows: number) => void,
	find: () => void,
	counted: (at: number, of: number) => void,
) {
	const { terminal, common } = build({ scrollback: 5000 });
	const search = new SearchAddon();

	terminal.loadAddon(search);
	search.onDidChangeResults(({ resultIndex, resultCount }) => counted(resultIndex, resultCount));
	terminal.onData(typed);
	terminal.onResize(({ cols, rows }) => sized(cols, rows));
	terminal.attachCustomKeyEventHandler((event) => {
		const finds =
			(event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "f";

		if (finds && event.type === "keydown") {
			event.preventDefault();
			find();
		}

		return !finds;
	});

	return {
		...common,
		// The bytes are a view into WebAssembly memory, valid only for this
		// call. xterm writes asynchronously, so it gets a copy.
		write: (printed: Uint8Array) => terminal.write(printed.slice()),
		reset: () => terminal.reset(),
		clear: () => terminal.clear(),
		find: (text: string, backwards: boolean) => {
			const marks = {
				decorations: {
					matchBackground: token("--chosen"),
					activeMatchBackground: token("--accent-fill"),
					matchOverviewRuler: token("--muted"),
					activeMatchColorOverviewRuler: token("--accent"),
				},
			};

			return backwards ? search.findPrevious(text, marks) : search.findNext(text, marks);
		},
		endFind: () => {
			search.clearDecorations();
			terminal.clearSelection();
		},
	};
}

/**
 * The page scrolls the log itself and hands over the visible lines in full on
 * each draw, so the terminal keeps no scrollback.
 */
function log() {
	const { terminal, common } = build({
		scrollback: 0,
		disableStdin: true,
		cursorInactiveStyle: "none",
	});

	// Hides the cursor and turns off line wrap, so long lines are cut.
	terminal.write("\x1b[?25l\x1b[?7l");
	// The page scrolls the log, so xterm must not take the wheel.
	common.host.addEventListener("wheel", (event) => event.stopImmediatePropagation(), {
		capture: true,
	});

	return {
		...common,
		draw: (lines: string) => terminal.write(lines),
	};
}

Object.assign(globalThis, { tetherTerminal: { shell, log } });
