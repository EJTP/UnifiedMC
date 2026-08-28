// Every accent carries white text, so every accent has to be dark enough to hold it.
//
// The presets shipped once with five of six action colours failing this - picked to be loud
// rather than readable, on the button people press most. Run by CI:
//
//   node scripts/check-contrast.mjs

import fs from "node:fs";

/** WCAG's threshold for normal-size text. The labels here are not large text. */
const READABLE = 4.5;

const source = fs.readFileSync(new URL("../src/lib/theme.ts", import.meta.url), "utf8");
const presets = [
	...source.matchAll(/id: "(\w+)", key: "[^"]+", primary: "(#[0-9a-f]{6})", cta: "(#[0-9a-f]{6})"/gi)
];

if (presets.length === 0) {
	console.error("no accents found - has the shape of ACCENTS changed?");
	process.exit(1);
}

const channel = (v) => {
	const c = v / 255;
	return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};

const whiteOn = (hex) => {
	const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
	return 1.05 / (0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b) + 0.05);
};

let failed = 0;
for (const [, id, primary, cta] of presets) {
	for (const [what, hex] of [["primary", primary], ["cta", cta]]) {
		const ratio = whiteOn(hex);
		const ok = ratio >= READABLE;
		if (!ok) failed++;
		console.log(`  ${ok ? "ok  " : "FAIL"} ${id}.${what.padEnd(7)} ${hex}  ${ratio.toFixed(2)}`);
	}
}

if (failed > 0) {
	console.error(`\n${failed} accent colour(s) cannot hold white text at ${READABLE}:1.`);
	process.exit(1);
}
console.log(`\nall ${presets.length} accents hold white text at ${READABLE}:1.`);
