<script lang="ts">
	import { Play, Package, Loader2, Trash2 } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import StatusDot from "./StatusDot.svelte";
	import type { SavedServer, ServerStatus } from "$lib/types";
	import type { ServerState } from "./StatusDot.svelte";

	let {
		server,
		status,
		hubVersion,
		busy = false,
		onplay,
		onmods,
		onremove
	}: {
		server: SavedServer;
		status?: ServerStatus;
		hubVersion: string;
		busy?: boolean;
		onplay: () => void;
		onmods: () => void;
		onremove: () => void;
	} = $props();

	const state = $derived<ServerState>(
		!status ? "checking" : !status.online ? "offline" : !status.manifest ? "unknown" : "ready"
	);

	const manifest = $derived(status?.manifest ?? null);

	/** What the row says about itself. Version, what it runs, how much it will pull. */
	const facts = $derived.by(() => {
		if (!status) return "wird geprüft …";
		if (!status.online) return status.error ?? "nicht erreichbar";
		if (!manifest) return "erreichbar · veröffentlicht kein Manifest";

		const parts = [manifest.minecraft];
		if (manifest.loader) parts.push(manifest.loader.type);
		if (manifest.mods.length) parts.push(`${manifest.mods.length} Mods`);
		if (manifest.config.length) parts.push(`${manifest.config.length} Configs`);
		return parts.join("  ·  ");
	});

	const canPlay = $derived(Boolean(status?.online && manifest));
</script>

<article
	class="group flex items-center gap-3.5 rounded-lg border border-border/70 bg-card px-4 py-3
	       transition-colors duration-200 hover:border-border hover:bg-accent/40"
>
	<StatusDot {state} />

	<div class="min-w-0 flex-1">
		<div class="flex items-baseline gap-2">
			<h3 class="truncate text-base font-medium">{server.name}</h3>
			{#if status?.online}
				<span class="shrink-0 font-mono text-xs text-muted-foreground">
					{status.players}/{status.max_players}
				</span>
			{/if}
		</div>
		<p class="truncate font-mono text-xs text-muted-foreground/70">{server.address}</p>
		<p class="mt-0.5 truncate text-xs text-muted-foreground">{facts}</p>
	</div>

	<Button
		variant="ghost"
		size="icon"
		class="size-8 text-muted-foreground opacity-0 transition-opacity
		       group-hover:opacity-100 hover:text-destructive focus-visible:opacity-100"
		onclick={onremove}
		aria-label="{server.name} entfernen"
	>
		<Trash2 class="size-4" />
	</Button>

	<Button
		variant="ghost"
		size="sm"
		class="h-9 text-muted-foreground hover:text-foreground"
		onclick={onmods}
		disabled={!canPlay}
	>
		<Package class="size-4" />
		Mods
	</Button>

	<Button
		size="sm"
		class="h-9 min-w-28 bg-cta text-cta-foreground hover:bg-cta/90"
		onclick={onplay}
		disabled={busy || !canPlay}
	>
		{#if busy}
			<Loader2 class="size-4 animate-spin" />
			Startet
		{:else}
			<Play class="size-4 fill-current" />
			Spielen
		{/if}
	</Button>
</article>
