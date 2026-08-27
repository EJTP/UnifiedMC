<script lang="ts">
	import { t } from "$lib/i18n.svelte";

	let {
		/** Seconds per day, keyed by whole days since the epoch. */
		days,
		/** Which day the run ends on, so the chart ends on today and not on the last session. */
		today,
		span = 14,
		height = 30
	}: {
		days: Record<number, number>;
		today: number;
		span?: number;
		height?: number;
	} = $props();

	/**
	 * Drawn in real pixels, with the viewBox matching the element one to one.
	 *
	 * A fixed viewBox plus `preserveAspectRatio="none"` is the shorter way to write this and
	 * the wrong one: it scales x and y by different factors, so a 2px gap comes out 4px wide
	 * and a round corner comes out an ellipse. Measuring costs one binding and keeps every
	 * spec below meaning what it says.
	 */
	let width = $state(0);

	/** A 2px gap in the surface colour is what separates neighbouring bars - never a stroke. */
	const GAP = 2;
	/** Never fill the slot; the leftover is air. */
	const MAX_BAR = 24;

	const slot = $derived(width / span);
	const barWidth = $derived(Math.min(MAX_BAR, Math.max(1, slot - GAP)));

	const buckets = $derived(
		Array.from({ length: span }, (_, i) => {
			const day = today - (span - 1 - i);
			return { day, seconds: days[day] ?? 0 };
		})
	);

	const peak = $derived(Math.max(...buckets.map((b) => b.seconds), 1));

	/**
	 * A rounded data-end and a square baseline, which a plain `rect` cannot do - `ry` would
	 * round both ends. The radius is capped by the bar's own width and height so a one-pixel
	 * day does not turn into a lozenge.
	 */
	function bar(x: number, top: number, width: number, bottom: number): string {
		const h = bottom - top;
		const r = Math.min(2, width / 2, h);
		return (
			`M${x},${bottom} L${x},${top + r} Q${x},${top} ${x + r},${top} ` +
			`L${x + width - r},${top} Q${x + width},${top} ${x + width},${top + r} ` +
			`L${x + width},${bottom} Z`
		);
	}

	/** Whole hours and minutes; a tooltip is read, not parsed. */
	function spell(seconds: number): string {
		if (seconds === 0) return t("played.none");
		const hours = Math.floor(seconds / 3600);
		const minutes = Math.round((seconds % 3600) / 60);
		if (hours === 0) return t("played.minutes", { minutes: Math.max(1, minutes) });
		return minutes === 0
			? t("played.hours", { hours })
			: t("played.hoursMinutes", { hours, minutes });
	}

	function label(day: number): string {
		// The bucket is a day number; a date is what a person recognises.
		return new Date(day * 86_400_000).toLocaleDateString(undefined, {
			weekday: "short",
			day: "numeric",
			month: "short"
		});
	}
</script>

<div bind:clientWidth={width} class="w-full">
	<!--
		One series, so no legend: the block's own label says what is plotted. No axis either -
		the shape is the message, and the tooltip carries every value the eye cannot read off it.
	-->
	<svg
		width={width}
		{height}
		viewBox="0 0 {width} {height}"
		role="img"
		aria-label={t("played.trend", { days: span })}
	>
		{#each buckets as bucket, i (bucket.day)}
			{@const x = i * slot}
			{@const isToday = bucket.day === today}
			{@const full = (bucket.seconds / peak) * (height - 2)}
			<!--
				A day with nothing played keeps a hairline rather than disappearing, so the run of
				days stays legible as a run - a gap would read as missing data, not a night off.
			-->
			{@const top = height - Math.max(full, bucket.seconds > 0 ? 2 : 1)}
			<path
				d={bar(x, top, barWidth, height)}
				fill={isToday ? "var(--primary)" : "color-mix(in oklab, var(--primary) 42%, transparent)"}
				opacity={bucket.seconds > 0 ? 1 : 0.25}
			>
				<title>{label(bucket.day)} · {spell(bucket.seconds)}</title>
			</path>
		{/each}
	</svg>
</div>
