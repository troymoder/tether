// The fonts are bundled so the page looks the same everywhere and fetches
// nothing from other hosts.

import inter from "@fontsource-variable/inter/files/inter-latin-wght-normal.woff2";
import monoItalic from "@fontsource-variable/jetbrains-mono/files/jetbrains-mono-latin-wght-italic.woff2";
import mono from "@fontsource-variable/jetbrains-mono/files/jetbrains-mono-latin-wght-normal.woff2";

const faces = [
	new FontFace("Inter", `url(${inter})`, { weight: "100 900" }),
	new FontFace("JetBrains Mono", `url(${mono})`, { weight: "100 800" }),
	new FontFace("JetBrains Mono", `url(${monoItalic})`, { weight: "100 800", style: "italic" }),
];

export const loaded = Promise.all(
	faces.map(async (face) => {
		document.fonts.add(await face.load());
	}),
).catch(() => {
	// The system fonts stand in when these fail to load.
});
