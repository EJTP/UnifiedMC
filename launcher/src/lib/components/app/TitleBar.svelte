<script lang="ts">
	import { Settings, Minus, X } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { launcher } from "$lib/state.svelte";

	let { onsettings }: { onsettings?: () => void } = $props();
</script>

<!--
	data-tauri-drag-region makes the bar itself the window handle. Buttons inside it
	must opt out, or dragging from them moves the window instead of clicking.
-->
<header
	data-tauri-drag-region
	class="flex h-12 shrink-0 items-center gap-3 border-b border-border/60 bg-card/40 px-3 backdrop-blur"
>
	<img src="/mark.png" alt="" class="pointer-events-none size-6" />
	<span class="pointer-events-none text-[13px] font-semibold tracking-tight">UnifiedMC</span>

	<div class="flex-1"></div>

	{#if launcher.account}
		<span class="hidden text-xs text-muted-foreground sm:inline">
			{launcher.account.name}
			{#if launcher.account.kind === "offline"}
				<span class="ml-1 rounded bg-warn/15 px-1.5 py-0.5 text-[10px] text-warn">offline</span>
			{/if}
		</span>
	{/if}

	<Button variant="ghost" size="icon" class="size-8" onclick={onsettings} aria-label="Einstellungen">
		<Settings class="size-4" />
	</Button>
</header>
