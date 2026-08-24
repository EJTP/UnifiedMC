<script lang="ts">
	import { Dialog as DialogPrimitive } from "bits-ui";
	import { Button } from "$lib/components/ui/button/index.js";
	import { cn, type WithElementRef } from "$lib/utils.js";
	import { t } from "$lib/i18n.svelte";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		ref = $bindable(null),
		class: className,
		children,
		showCloseButton = false,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
		showCloseButton?: boolean;
	} = $props();
</script>

<!--
	Sticky, and opaque rather than tinted: the dialog box is what scrolls when its content is
	taller than the window, and a footer that scrolls with it leaves the player looking at a
	half-cut confirm button. -bottom-4 cancels the -mb-4 the bar hangs by, so it lands on the
	dialog's bottom edge instead of one padding step above it.
-->
<div
	bind:this={ref}
	data-slot="dialog-footer"
	class={cn("sticky -bottom-4 z-10 -mx-4 -mb-4 rounded-b-xl border-t bg-muted p-4 flex flex-col-reverse gap-2 sm:flex-row sm:justify-end", className)}
	{...restProps}
>
	{@render children?.()}
	{#if showCloseButton}
		<DialogPrimitive.Close>
			{#snippet child({ props })}
				<Button variant="outline" {...props}>{t("common.close")}</Button>
			{/snippet}
		</DialogPrimitive.Close>
	{/if}
</div>
