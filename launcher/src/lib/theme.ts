/**
 * What the window can be painted in.
 *
 * Two axes, because they answer different questions. The accent is the pair of colours that
 * carry action - buttons, focus rings, the glow behind the window. The backdrop is what
 * everything sits on. Everything in `app.css` is already written in terms of these custom
 * properties, so a theme is a handful of hex values rather than a second stylesheet.
 */

export interface Accent {
	id: string;
	/** Named in the interface, so it is a word rather than a hex code. */
	key: string;
	/**
	 * Buttons, focus rings, the first ambient wash. White sits on this, so every preset is a
	 * 600/700 step: a 500 looks brighter and fails contrast against its own label.
	 */
	primary: string;
	/**
	 * The single loud action per screen, and the second ambient wash.
	 *
	 * Also a 600/700 step, for the same reason as `primary`: the Play button is the most
	 * pressed control in the launcher and its label is white. Five of these were once picked
	 * to be loud rather than readable, and every one of them failed contrast against its own
	 * text - which `scripts/check-contrast.mjs` now refuses to let happen again.
	 */
	cta: string;
}

export const ACCENTS: Accent[] = [
	// The default is exactly what the launcher already looked like, so choosing nothing
	// changes nothing.
	{ id: "violet", key: "accent.violet", primary: "#7c3aed", cta: "#e11d48" },
	{ id: "ocean", key: "accent.ocean", primary: "#0369a1", cta: "#b45309" },
	{ id: "forest", key: "accent.forest", primary: "#047857", cta: "#c2410c" },
	{ id: "ember", key: "accent.ember", primary: "#c2410c", cta: "#e11d48" },
	{ id: "rose", key: "accent.rose", primary: "#be123c", cta: "#7c3aed" },
	{ id: "indigo", key: "accent.indigo", primary: "#4338ca", cta: "#0e7490" }
];

/**
 * The surfaces everything is drawn on, darkest first.
 *
 * Each is a full set rather than a tint applied to one base: a backdrop that only shifted the
 * background would leave cards and borders belonging to the old one, and the seams show.
 */
export interface Backdrop {
	id: string;
	key: string;
	background: string;
	card: string;
	popover: string;
	secondary: string;
	muted: string;
	accent: string;
	border: string;
	surfaceTop: string;
	surfaceBottom: string;
}

export const BACKDROPS: Backdrop[] = [
	{
		id: "midnight",
		key: "backdrop.midnight",
		background: "#0f0f23",
		card: "#16162b",
		popover: "#1a1a33",
		secondary: "#27273b",
		muted: "#22223a",
		accent: "#2a2a44",
		border: "#262640",
		surfaceTop: "#1a1a30",
		surfaceBottom: "#14142a"
	},
	{
		id: "slate",
		key: "backdrop.slate",
		background: "#0e1116",
		card: "#151a21",
		popover: "#1a212a",
		secondary: "#232c37",
		muted: "#1e262f",
		accent: "#27313d",
		border: "#232c37",
		surfaceTop: "#171d25",
		surfaceBottom: "#12171e"
	},
	{
		// For an OLED panel, where a true black pixel is simply off.
		id: "black",
		key: "backdrop.black",
		background: "#000000",
		card: "#0b0b0d",
		popover: "#121215",
		secondary: "#1c1c20",
		muted: "#171719",
		accent: "#202024",
		border: "#1f1f24",
		surfaceTop: "#0e0e11",
		surfaceBottom: "#08080a"
	},
	{
		id: "plum",
		key: "backdrop.plum",
		background: "#17111d",
		card: "#1f1727",
		popover: "#251c2e",
		secondary: "#332741",
		muted: "#2b2136",
		accent: "#3a2c49",
		border: "#322648",
		surfaceTop: "#221a2c",
		surfaceBottom: "#1b1424"
	}
];

/** What the interface stores when the two colours below are the ones to use. */
export const CUSTOM = "custom";

export function accentOf(id: string): Accent {
	return ACCENTS.find((accent) => accent.id === id) ?? ACCENTS[0];
}

export function backdropOf(id: string): Backdrop {
	return BACKDROPS.find((backdrop) => backdrop.id === id) ?? BACKDROPS[0];
}

/** The chosen pair, whether it came from a preset or the colour pickers. */
export function pairOf(theme: {
	accent: string;
	accent_primary: string;
	accent_cta: string;
}): { primary: string; cta: string } {
	if (theme.accent === CUSTOM) {
		return { primary: theme.accent_primary, cta: theme.accent_cta };
	}
	const preset = accentOf(theme.accent);
	return { primary: preset.primary, cta: preset.cta };
}

/**
 * How readable white is on a colour, as a WCAG contrast ratio.
 *
 * Both accents carry white text, so a colour picked too light makes its own label vanish.
 * The interface says so rather than refusing the choice - it is the player's window.
 */
export function whiteContrast(hex: string): number {
	const channel = (v: number) => {
		const c = v / 255;
		return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
	};
	const clean = hex.replace("#", "");
	if (!/^[0-9a-f]{6}$/i.test(clean)) return 21;
	const [r, g, b] = [0, 2, 4].map((i) => parseInt(clean.slice(i, i + 2), 16));
	const luminance = 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
	return 1.05 / (luminance + 0.05);
}

/** Below this, white on the colour is not readable. WCAG's threshold for normal text. */
export const READABLE = 4.5;

/**
 * Put a theme into effect.
 *
 * On the root element rather than in the stylesheet: these are the same custom properties
 * `:root` declares, so setting them inline overrides the defaults without a second copy of the
 * palette to keep in step.
 */
export function applyTheme(theme: {
	accent: string;
	accent_primary: string;
	accent_cta: string;
	backdrop: string;
}) {
	if (typeof document === "undefined") return;
	const root = document.documentElement.style;
	const { primary, cta } = pairOf(theme);
	const surface = backdropOf(theme.backdrop);

	root.setProperty("--primary", primary);
	// No --ring: app.css mixes it up off --primary to clear the focus-ring contrast threshold,
	// and setting it inline here would overrule that with the unlightened accent.
	root.setProperty("--hue-a", primary);
	root.setProperty("--cta", cta);
	root.setProperty("--hue-b", cta);

	root.setProperty("--background", surface.background);
	root.setProperty("--card", surface.card);
	root.setProperty("--popover", surface.popover);
	root.setProperty("--secondary", surface.secondary);
	root.setProperty("--muted", surface.muted);
	root.setProperty("--accent", surface.accent);
	root.setProperty("--border", surface.border);
	root.setProperty("--input", surface.border);
	root.setProperty("--surface-top", surface.surfaceTop);
	root.setProperty("--surface-bottom", surface.surfaceBottom);
}
