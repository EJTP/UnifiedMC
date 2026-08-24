<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import CheckIcon from "@lucide/svelte/icons/check";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";
	import type { Snippet } from "svelte";

	// Renamed on the way in: the row below opens its own `children` snippet, and a prop of the
	// same name inside it would resolve to that snippet and render itself forever.
	let {
		ref = $bindable(null),
		class: className,
		value,
		label,
		hint,
		badge,
		mono = false,
		children: body,
		...restProps
	}: WithoutChildrenOrChild<SelectPrimitive.ItemProps> & {
		/** A second line under the label, for the consequence of picking this one. */
		hint?: string;
		/** A short pill on the right - "erkannt", "empfohlen". Not a sentence. */
		badge?: string;
		/** Versions and flags are read as strings of digits, and line up only in mono. */
		mono?: boolean;
		children?: Snippet;
	} = $props();
</script>

<SelectPrimitive.Item
	bind:ref
	{value}
	{label}
	data-slot="select-item"
	class={cn(
		"flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-sm outline-none transition-colors duration-150 select-none data-highlighted:bg-accent data-highlighted:text-accent-foreground data-selected:bg-primary/15 data-disabled:pointer-events-none data-disabled:opacity-50",
		className
	)}
	{...restProps}
>
	{#snippet children({ selected })}
		<span class="min-w-0 flex-1">
			<span class={cn("block truncate", mono && "font-mono")}>
				{#if body}{@render body()}{:else}{label ?? value}{/if}
			</span>
			{#if hint}
				<span class="mt-0.5 block truncate text-xs text-muted-foreground">{hint}</span>
			{/if}
		</span>

		{#if badge}
			<span
				class="shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[0.65rem] font-medium text-muted-foreground"
			>
				{badge}
			</span>
		{/if}

		<!-- The tick keeps its space either way, so the row does not shift on selection. -->
		<CheckIcon class={cn("size-4 shrink-0 text-primary", !selected && "invisible")} />
	{/snippet}
</SelectPrimitive.Item>
