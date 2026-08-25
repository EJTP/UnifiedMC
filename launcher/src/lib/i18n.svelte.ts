import de from "./i18n/de";
import en from "./i18n/en";

export type Locale = "de" | "en";

export const LOCALES: { id: Locale; label: string }[] = [
	{ id: "de", label: "Deutsch" },
	{ id: "en", label: "English" }
];

const dicts: Record<Locale, Record<string, string>> = { de, en };

/** The language the window is drawn in. An object, because an exported `$state` cannot be reassigned. */
export const locale = $state<{ current: Locale }>({ current: "de" });

/** Whether the German dict - the one that defines the key inventory - knows this key. */
export function has(key: string): boolean {
	return key in de;
}

export function t(key: string, vars?: Record<string, string | number>): string {
	const text = dicts[locale.current][key] ?? (de as Record<string, string>)[key] ?? key;
	if (!vars) return text;
	return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
		name in vars ? String(vars[name]) : whole
	);
}

/**
 * What the backend sent, in the player's language when it named a key. Rust reports progress
 * phases and its own errors as dotted keys; anything else is shown as it came.
 */
export function translate(text: string): string {
	return has(text) ? t(text) : text;
}

/** "system" means whatever the machine is set to; anything else is an explicit choice. */
export function resolveLocale(language: string): Locale {
	if (language === "de" || language === "en") return language;
	return typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("de")
		? "de"
		: "en";
}
