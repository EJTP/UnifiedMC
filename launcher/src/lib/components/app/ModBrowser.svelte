<script lang="ts">
	import { ArrowLeft, Check, Download, Loader2, Package, Trash2 } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import { launcher, type Tab } from "$lib/state.svelte";
	import type { Hit } from "$lib/types";

	let query = $state("");
	let timer: ReturnType<typeof setTimeout>;

	const tabs: { id: Tab; label: string }[] = [
		{ id: "search", label: "Katalog" },
		{ id: "installed", label: "Installiert" },
		{ id: "pack", label: "Im Pack" }
	];

	const hint = $derived(
		launcher.tab === "search"
			? "Nur was ohne Server-Seite läuft. Grau: kommt schon mit dem Pack."
			: launcher.tab === "installed"
				? "Deine eigenen Mods für diesen Server."
				: "Alles, was der Server ausliefert."
	);

	/** Typing should not fire a request per keystroke; the catalogue is a network away. */
	function search(value: string) {
		clearTimeout(timer);
		timer = setTimeout(() => void launcher.loadMods(value), 220);
	}

	function compact(count: number) {
		if (count >= 1_000_000) return `${Math.round(count / 1_000_000)}M`;
		if (count >= 1_000) return `${Math.round(count / 1_000)}k`;
		return String(count);
	}

	function selectable(hit: Hit) {
		return !hit.on_server && launcher.tab !== "pack";
	}
</script>

<!--
	flex-1 with min-h-0, not h-full: as a flex child, h-full resolves against a height this
	element also influences, and the scroll area's resize observer then chases its own tail
	until the renderer gives up.
-->
<div class="flex min-h-0 flex-1 flex-col">
	<div class="flex items-center gap-2.5 px-5 pt-4">
		<Button variant="ghost" size="icon" class="size-9" onclick={() => launcher.closeMods()}>
			<ArrowLeft class="size-4" />
		</Button>
		<div class="min-w-0">
			<h1 class="truncate text-sm font-semibold">Mods</h1>
			<p class="truncate font-mono text-xs text-muted-foreground">
				{launcher.browsing?.address}
			</p>
		</div>
	</div>

	<div class="flex items-center gap-1.5 px-5 pt-3">
		{#each tabs as tab (tab.id)}
			<Button
				variant={launcher.tab === tab.id ? "secondary" : "ghost"}
				size="sm"
				class="h-9 text-xs {launcher.tab === tab.id ? '' : 'text-muted-foreground'}"
				onclick={() => launcher.switchTab(tab.id)}
			>
				{tab.label}
			</Button>
		{/each}
	</div>

	<p class="px-5 pt-2.5 text-xs text-muted-foreground">{hint}</p>

	{#if launcher.tab === "search"}
		<div class="px-5 pt-3">
			<Input
				bind:value={query}
				oninput={() => search(query)}
				placeholder="Mod suchen …"
				class="h-9"
			/>
		</div>
	{/if}

	<div class="mt-3 min-h-0 flex-1 overflow-y-auto px-5">
		<div class="flex flex-col gap-1.5 pb-4">
			{#each launcher.hits as hit (hit.id)}
				{@const chosen = launcher.picked.has(hit.id)}
				<button
					type="button"
					disabled={!selectable(hit)}
					onclick={() => launcher.toggle(hit.id)}
					class="flex items-center gap-3 rounded-lg border px-3.5 py-2.5 text-left transition-colors
					       duration-150 disabled:cursor-default
					       {chosen
						? 'border-primary/60 bg-primary/15'
						: hit.on_server
							? 'border-border/50 bg-muted/40'
							: 'border-border/70 bg-card hover:border-border hover:bg-accent/40'}"
				>
					<div
						class="flex size-11 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted"
					>
						{#if hit.icon}
							<!-- pixel art, mostly: smoothing it on the way down makes it mush -->
							<img
								src={hit.icon}
								alt=""
								loading="lazy"
								class="size-full object-cover [image-rendering:auto]"
							/>
						{:else}
							<Package class="size-4 text-muted-foreground/60" />
						{/if}
					</div>

					<div class="min-w-0 flex-1">
						<p class="truncate text-sm {hit.on_server ? 'text-muted-foreground' : ''}">
							{hit.title}
						</p>
						{#if hit.description}
							<p class="truncate text-xs text-muted-foreground/70">{hit.description}</p>
						{/if}
					</div>

					{#if hit.on_server}
						<span class="flex shrink-0 items-center gap-1.5 text-xs text-ok">
							<Check class="size-4" />
							im Pack
						</span>
					{:else if launcher.tab === "installed"}
						<span class="shrink-0 text-xs text-muted-foreground">
							{chosen ? "wird entfernt" : ""}
						</span>
					{:else}
						<span class="shrink-0 font-mono text-xs text-muted-foreground/70">
							{compact(hit.downloads)}
							{hit.source === "modrinth" ? "MR" : "CF"}
						</span>
					{/if}
				</button>
			{/each}

			{#if launcher.hits.length === 0 && !launcher.loadingMods}
				<p class="py-10 text-center text-sm text-muted-foreground">
					{launcher.note || "Nichts hier"}
				</p>
			{/if}

			{#if launcher.more}
				<Button
					variant="ghost"
					class="mt-1 w-full text-muted-foreground"
					disabled={launcher.loadingMods}
					onclick={() => launcher.loadMore()}
				>
					{launcher.loadingMods ? "Lädt …" : "Mehr laden"}
				</Button>
			{/if}
		</div>
	</div>

	<div class="flex items-center gap-3 border-t border-border/60 px-5 py-3">
		<span class="flex-1 truncate text-xs {launcher.note ? 'text-foreground' : 'text-muted-foreground'}">
			{#if launcher.loadingMods}
				<span class="flex items-center gap-2">
					<Loader2 class="size-4 animate-spin" /> Lädt …
				</span>
			{:else if launcher.picked.size > 0}
				{launcher.picked.size} ausgewählt
			{:else}
				{launcher.note}
			{/if}
		</span>

		{#if launcher.tab !== "pack"}
			<Button
				size="sm"
				class="h-8"
				variant={launcher.tab === "installed" ? "destructive" : "default"}
				disabled={launcher.picked.size === 0 || launcher.loadingMods}
				onclick={() => launcher.applyMods()}
			>
				{#if launcher.tab === "installed"}
					<Trash2 class="size-4" />
					Entfernen
				{:else}
					<Download class="size-4" />
					Installieren
				{/if}
			</Button>
		{/if}
	</div>
</div>
