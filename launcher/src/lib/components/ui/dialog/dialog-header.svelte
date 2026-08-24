<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<!-- pr-8: the close button sits in that corner, and a long title may not run under it. -->
<div
	bind:this={ref}
	data-slot="dialog-header"
	class={cn("gap-2 flex flex-col pr-8", className)}
	{...restProps}
>
	{@render children?.()}
</div>
