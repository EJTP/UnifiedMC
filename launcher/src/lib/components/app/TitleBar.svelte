<script lang="ts">
	import { Settings } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { launcher } from "$lib/state.svelte";

	let { onsettings }: { onsettings: () => void } = $props();
</script>

<!--
	data-tauri-drag-region turns the bar into the window handle. Anything clickable inside it
	has to sit above that, or dragging from a button moves the window instead of pressing it.
-->
<header
	data-tauri-drag-region
	class="flex h-11 shrink-0 items-center gap-2.5 border-b border-border/60 bg-card/50 px-3"
>
	<img src="/mark.png" alt="" class="pointer-events-none size-5" />
	<span class="pointer-events-none text-base font-semibold">UnifiedMC</span>

	<div class="flex-1"></div>

	{#if launcher.session}
		<span class="flex items-center gap-1.5 text-xs text-muted-foreground">
			{launcher.session.name}
			{#if launcher.session.kind === "offline"}
				<span
					class="rounded bg-warn/15 px-1.5 py-0.5 text-xs font-medium text-warn"
					title="Ohne Anmeldung erreichst du nur Server im Offline-Modus"
				>
					offline
				</span>
			{/if}
		</span>
	{/if}

	<Button variant="ghost" size="icon" class="size-8" onclick={onsettings} aria-label="Einstellungen">
		<Settings class="size-4" />
	</Button>
</header>
