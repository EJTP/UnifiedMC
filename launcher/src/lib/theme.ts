/**
 * The accents the window can be painted in.
 *
 * Only two colours each. Everything else in `app.css` is already expressed in terms of them -
 * the ambient wash, the card hover ring, the strong chip, the focus ring - so an accent is a
 * pair of hex values rather than a stylesheet.
 */
export interface Accent {
	id: string;
	/** Named in the interface, so it is a word rather than a hex code. */
	key: string;
	/**
	 * Buttons, focus rings, the first ambient wash. White sits on this, so every one of these
	 * is a 600/700 step: a 500 would look brighter and fail contrast against its own label.
	 */
	primary: string;
	/** The single loud action per screen, and the second ambient wash. */
	cta: string;
}

export const ACCENTS: Accent[] = [
	// The default is exactly what the launcher already looked like, so choosing nothing
	// changes nothing.
	{ id: "violet", key: "accent.violet", primary: "#7c3aed", cta: "#f43f5e" },
	{ id: "ocean", key: "accent.ocean", primary: "#0369a1", cta: "#f59e0b" },
	{ id: "forest", key: "accent.forest", primary: "#047857", cta: "#f97316" },
	{ id: "ember", key: "accent.ember", primary: "#c2410c", cta: "#e11d48" },
	{ id: "rose", key: "accent.rose", primary: "#be123c", cta: "#8b5cf6" },
	{ id: "indigo", key: "accent.indigo", primary: "#4338ca", cta: "#06b6d4" }
];

export function accentOf(id: string): Accent {
	return ACCENTS.find((accent) => accent.id === id) ?? ACCENTS[0];
}

/**
 * Put an accent into effect.
 *
 * On the root element rather than in the stylesheet: these are the same custom properties
 * `:root` declares, so setting them inline overrides the defaults without a second copy of
 * the palette to keep in step.
 */
export function applyAccent(id: string) {
	if (typeof document === "undefined") return;
	const accent = accentOf(id);
	const root = document.documentElement.style;

	root.setProperty("--primary", accent.primary);
	root.setProperty("--ring", accent.primary);
	root.setProperty("--hue-a", accent.primary);
	root.setProperty("--cta", accent.cta);
	root.setProperty("--hue-b", accent.cta);
}
