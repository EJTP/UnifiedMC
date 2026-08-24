<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
	import { cn, type WithoutChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithoutChild<SelectPrimitive.TriggerProps> = $props();
</script>

<!--
	A filled surface rather than a hairline box: on the popover background the input border
	alone is almost invisible, which is what made these read as disabled fields.
-->
<SelectPrimitive.Trigger
	bind:ref
	data-slot="select-trigger"
	class={cn(
		"group flex h-9 w-full min-w-0 cursor-pointer items-center justify-between gap-2 rounded-lg border border-input bg-input/40 px-3 py-1 text-sm outline-none transition-colors select-none hover:bg-input/70 hover:border-border focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 data-[state=open]:border-ring data-[state=open]:bg-input/70 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 data-placeholder:text-muted-foreground disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 [&>span]:truncate",
		className
	)}
	{...restProps}
>
	{@render children?.()}
	<ChevronDownIcon
		class="size-4 shrink-0 text-muted-foreground transition-transform duration-200 group-data-[state=open]:rotate-180"
	/>
</SelectPrimitive.Trigger>
