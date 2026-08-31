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

const luminance = (hex) => {
	const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
	return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
};

const contrast = (a, b) => {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
};

const whiteOn = (hex) => contrast("#ffffff", hex);

let failed = 0;
for (const [, id, primary, cta] of presets) {
	for (const [what, hex] of [["primary", primary], ["cta", cta]]) {
		const ratio = whiteOn(hex);
		const ok = ratio >= READABLE;
		if (!ok) failed++;
		console.log(`  ${ok ? "ok  " : "FAIL"} ${id}.${what.padEnd(7)} ${hex}  ${ratio.toFixed(2)}`);
	}
}

// The muted tiers are two fixed hexes over four backdrops, so the only honest number is the
// worst pairing. Scraped from the two files rather than duplicated here: a table of colours
// kept in step by hand is the thing that let five bad accents ship in the first place.
const css = fs.readFileSync(new URL("../src/app.css", import.meta.url), "utf8");
const backdrops = [...source.matchAll(/id: "(\w+)",\s*key: "backdrop\.[^"]+",([^}]*)}/g)];

if (backdrops.length === 0) {
	console.error("no backdrops found - has the shape of BACKDROPS changed?");
	process.exit(1);
}

for (const name of ["muted-foreground", "muted-foreground-dim"]) {
	const token = css.match(new RegExp(`--${name}:\\s*(#[0-9a-f]{6})`, "i"));
	if (!token) {
		console.error(`--${name} is not a hex in app.css - renamed, or moved out of :root?`);
		process.exit(1);
	}
	let worst = { ratio: Infinity, on: "" };
	for (const [, id, body] of backdrops) {
		for (const [, surface, hex] of body.matchAll(/(\w+): "(#[0-9a-f]{6})"/gi)) {
			const ratio = contrast(token[1], hex);
			if (ratio < worst.ratio) worst = { ratio, on: `${id}.${surface}` };
		}
	}
	const ok = worst.ratio >= READABLE;
	if (!ok) failed++;
	const label = `  ${ok ? "ok  " : "FAIL"} ${name.padEnd(20)} ${token[1]}`;
	console.log(`${label}  ${worst.ratio.toFixed(2)} on ${worst.on}`);
}

if (failed > 0) {
	console.error(`\n${failed} colour(s) fall under ${READABLE}:1.`);
	process.exit(1);
}
console.log(
	`\nall ${presets.length} accents hold white text, and muted text reads on all ` +
		`${backdrops.length} backdrops, at ${READABLE}:1.`
);
