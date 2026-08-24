<script lang="ts">
	import { Play, Package, Loader2 } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import StatusDot from "./StatusDot.svelte";
	import type { Server } from "$lib/types";

	let {
		server,
		busy = false,
		onplay,
		onmods
	}: { server: Server; busy?: boolean; onplay?: () => void; onmods?: () => void } = $props();

	const facts = $derived(
		[
			server.minecraft,
			server.loader || null,
			server.mods ? `${server.mods} Mods` : null,
			server.config ? `${server.config} Configs` : null
		].filter(Boolean) as string[]
	);

	const note = $derived(
		server.state === "offline"
			? "nicht erreichbar"
			: server.state === "checking"
				? "wird geprüft"
				: server.state === "ready"
					? "startet sofort"
					: "startet neu"
	);
</script>

<article
	class="group relative flex items-center gap-4 rounded-lg border border-border/70 bg-card px-4 py-3.5
	       transition-colors duration-200 hover:border-border hover:bg-accent/40"
>
	<StatusDot state={server.state} />

	<div class="min-w-0 flex-1">
		<div class="flex items-baseline gap-2">
			<h3 class="truncate text-sm font-medium">{server.name}</h3>
			{#if server.online !== undefined}
				<span class="shrink-0 font-mono text-[11px] text-muted-foreground">
					{server.online}/{server.max}
				</span>
			{/if}
		</div>

		<p class="truncate font-mono text-[11px] text-muted-foreground/80">{server.address}</p>

		<p class="mt-1 truncate text-xs text-muted-foreground">
			{#if facts.length}
				{facts.join("  ·  ")}<span class="text-muted-foreground/60">  ·  {note}</span>
			{:else}
				{note}
			{/if}
		</p>
	</div>

	<div class="flex shrink-0 items-center gap-2">
		<Button
			variant="ghost"
			size="sm"
			class="h-8 text-muted-foreground hover:text-foreground"
			onclick={onmods}
			disabled={server.state === "offline" || server.state === "checking"}
		>
			<Package class="size-3.5" />
			Mods
		</Button>

		<Button
			size="sm"
			class="h-8 min-w-[92px] bg-cta text-cta-foreground hover:bg-cta/90"
			onclick={onplay}
			disabled={busy || server.state === "offline" || server.state === "checking"}
		>
			{#if busy}
				<Loader2 class="size-3.5 animate-spin" />
				Startet
			{:else}
				<Play class="size-3.5 fill-current" />
				Spielen
			{/if}
		</Button>
	</div>
</article>
