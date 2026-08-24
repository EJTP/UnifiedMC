<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn, type WithoutChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		sideOffset = 6,
		collisionPadding = 8,
		portalProps,
		children,
		...restProps
	}: WithoutChild<SelectPrimitive.ContentProps> & {
		portalProps?: SelectPrimitive.PortalProps;
	} = $props();
</script>

<!--
	Portalled: inside a dialog the list would otherwise be clipped by the dialog's own box,
	and a picker near the bottom edge would open into nothing.
-->
<SelectPrimitive.Portal {...portalProps}>
	<SelectPrimitive.Content
		bind:ref
		{sideOffset}
		{collisionPadding}
		data-slot="select-content"
		class={cn(
			"z-50 min-w-[var(--bits-floating-anchor-width)] overflow-hidden rounded-xl border border-border/80 bg-popover p-1 text-sm text-popover-foreground shadow-2xl ring-1 ring-background/60 outline-none duration-100 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=closed]:animate-out data-[state=closed]:fade-out-0",
			className
		)}
		{...restProps}
	>
		<!--
			The scrolling lives on the viewport, never on the content. The content is what
			floating-ui measures to place and size the popover, so giving it its own scrollbar
			feeds the measurement back into itself - the renderer then re-measures until it
			stops answering. bits-ui hides the viewport's scrollbar for us.
		-->
		<!--
			The available height floating-ui reports is for the whole popover, and the box around
			this viewport still costs its own padding and hairline border. Handing the raw number
			to the viewport makes the popover exactly that much taller than the space it was told
			it had, and a list opened upwards then runs off the top of the window.
		-->
		<SelectPrimitive.Viewport
			class="max-h-[min(18rem,calc(var(--bits-floating-available-height)-0.5rem-2px))] w-full scroll-my-1 overflow-y-auto"
		>
			{@render children?.()}
		</SelectPrimitive.Viewport>
	</SelectPrimitive.Content>
</SelectPrimitive.Portal>
