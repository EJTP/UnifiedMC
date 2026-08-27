import { t } from "./i18n.svelte";
import type { Played } from "./types";

/**
 * Playtime, the way a person says it. Rounded to the unit above whatever is being shown:
 * "184h" beside a server name, never "184h 12m 06s" - a row is scanned, not audited.
 */
export function spell(seconds: number): string {
	if (seconds <= 0) return t("played.none");
	const hours = Math.floor(seconds / 3600);
	const minutes = Math.round((seconds % 3600) / 60);
	if (hours === 0) return t("played.minutes", { minutes: Math.max(1, minutes) });
	// Past a day of play the minutes are noise; nobody reads the tail of "184h 12m".
	if (hours >= 24 || minutes === 0) return t("played.hours", { hours });
	return t("played.hoursMinutes", { hours, minutes });
}

/**
 * How long ago, in the largest unit that is still true. Not a date: "3 days ago" is what the
 * question "have I played this lately" actually wants, and it needs no locale format.
 */
export function ago(unixSeconds: number): string {
	if (!unixSeconds) return "";
	const seconds = Math.max(0, Math.floor(Date.now() / 1000) - unixSeconds);

	if (seconds < 3600) return t("played.justNow");
	const hours = Math.floor(seconds / 3600);
	if (hours < 24) return t("played.hoursAgo", { hours });
	const days = Math.floor(hours / 24);
	if (days === 1) return t("played.yesterday");
	if (days < 30) return t("played.daysAgo", { days });
	const months = Math.floor(days / 30);
	return months < 12
		? t("played.monthsAgo", { months })
		: t("played.yearsAgo", { years: Math.floor(days / 365) });
}

/**
 * Newest first, never played last.
 *
 * A stable sort, so two rows that have never been played keep the order they were added in -
 * a list that reshuffles itself on every render is worse than one that never sorts at all.
 */
export function byLastPlayed<T>(rows: T[], key: (row: T) => string, book: Record<string, Played>): T[] {
	return rows
		.map((row, index) => ({ row, index, last: book[key(row)]?.last ?? 0 }))
		.sort((a, b) => b.last - a.last || a.index - b.index)
		.map((entry) => entry.row);
}
