<script lang="ts">
	import { Minus, Plus } from "@lucide/svelte";
	import { cn } from "$lib/utils.js";

	/**
	 * A number field with its own stepper.
	 *
	 * The native spin buttons are a browser control: they ignore the border radius, they draw
	 * in the platform's colours rather than the window's, and they only appear on hover, so a
	 * form reflows under the cursor. They are turned off in `app.css` and replaced here.
	 */
	let {
		value = $bindable(0),
		min = 0,
		max = Number.MAX_SAFE_INTEGER,
		step = 1,
		id,
		class: className,
		...restProps
	}: {
		value?: number;
		min?: number;
		max?: number;
		step?: number;
		id?: string;
		class?: string;
		[key: string]: unknown;
	} = $props();

	const clamp = (n: number) => Math.min(max, Math.max(min, n));

	/**
	 * Step onto the grid, not off it: from 4000 with a step of 512 the next value up is 4096,
	 * not 4512. Anything typed by hand is left exactly as typed until a button is pressed.
	 */
	function nudge(direction: 1 | -1) {
		const from = Number.isFinite(value) ? value : min;
		const grid = Math.round((from - min) / step) * step + min;
		const next = Math.abs(grid - from) > 1e-9 && Math.sign(grid - from) === direction
			? grid
			: grid + direction * step;
		value = clamp(next);
	}

	/** An empty field reports NaN while it is being retyped; committing it settles it. */
	function settle() {
		value = Number.isFinite(value) ? clamp(value) : min;
	}
</script>

<div
	class={cn(
		"flex h-8 w-full items-center rounded-lg border border-input bg-transparent transition-colors",
		"focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50 dark:bg-input/30",
		className
	)}
>
	<input
		{id}
		type="number"
		bind:value
		{min}
		{max}
		{step}
		onblur={settle}
		class="h-full min-w-0 flex-1 bg-transparent px-2.5 py-1 font-mono text-sm outline-none
		       placeholder:text-muted-foreground"
		{...restProps}
	/>

	<!--
		Inside the field's own border rather than beside it: two separate buttons would make
		this three controls wide in a two-column grid, and the pair belongs to the number.
	-->
	<div class="flex h-full shrink-0 items-center border-l border-input">
		{#each [{ dir: -1 as const, icon: Minus, at: min }, { dir: 1 as const, icon: Plus, at: max }] as button (button.dir)}
			<button
				type="button"
				tabindex="-1"
				disabled={value === button.at}
				onclick={() => nudge(button.dir)}
				aria-label={button.dir === 1 ? "+" : "−"}
				class="flex h-full w-7 items-center justify-center text-muted-foreground outline-none
				       transition-colors hover:bg-accent/50 hover:text-foreground
				       disabled:pointer-events-none disabled:opacity-30
				       first:border-r first:border-input"
			>
				<button.icon class="size-3" />
			</button>
		{/each}
	</div>
</div>
