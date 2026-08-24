<script lang="ts">
	import { ArrowLeft, Check, Download, Loader2, Package, Trash2 } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import { t } from "$lib/i18n.svelte";
	import { launcher, type Tab } from "$lib/state.svelte";
	import type { Hit } from "$lib/types";
	import { cn } from "$lib/utils";

	let query = $state("");
	let timer: ReturnType<typeof setTimeout>;

	/** Icons come from two catalogues; a dead URL must not leave a broken-image glyph behind. */
	let broken = $state<Set<string>>(new Set());

	const tabs: { id: Tab; key: string }[] = [
		{ id: "search", key: "mods.tab.search" },
		{ id: "installed", key: "mods.tab.installed" },
		{ id: "pack", key: "mods.tab.pack" }
	];

	/**
	 * What this tab is showing - plus, in the catalogue, which catalogues are actually in it.
	 * Without a CurseForge key half the world of mods is silently missing, and a player who is
	 * not told reads that as "the search is broken".
	 */
	const hint = $derived(
		t(`mods.hint.${launcher.tab}`) +
			(launcher.tab === "search" && !launcher.settings.curseforge_key
				? ` ${t("mods.modrinthOnly")}`
				: "")
	);

	/** An instance is browsed through a synthetic "instance-<id>" address nobody should read. */
	const address = $derived(
		launcher.browsing && !launcher.browsing.address.startsWith("instance-")
			? launcher.browsing.address
			: ""
	);

	/** Typing should not fire a request per keystroke; the catalogue is a network away. */
	function search(value: string) {
		clearTimeout(timer);
		timer = setTimeout(() => void launcher.loadMods(value), 220);
	}

	function switchTab(tab: Tab) {
		clearTimeout(timer);
		query = "";
		void launcher.switchTab(tab);
	}

	function compact(count: number) {
		if (count >= 1_000_000) return `${Math.round(count / 1_000_000)}M`;
		if (count >= 1_000) return `${Math.round(count / 1_000)}k`;
		return String(count);
	}

	function selectable(hit: Hit) {
		// already in the pack or already in the profile: there is nothing to install
		return !hit.on_server && !hit.installed && launcher.tab !== "pack";
	}
</script>

<!--
	flex-1 with min-h-0, not h-full: as a flex child, h-full resolves against a height this
	element also influences, and the scroll area's resize observer then chases its own tail
	until the renderer gives up.
-->
<div class="flex min-h-0 flex-1 flex-col">
	<!-- h-11 and px-6, the same strip the list header uses: the back button lands where the
	     title it replaced was, instead of one line lower and half a step to the left. -->
	<div class="flex h-11 shrink-0 items-center gap-2.5 px-6">
		<Button
			variant="ghost"
			size="icon-lg"
			aria-label={t("common.back")}
			onclick={() => launcher.closeMods()}
		>
			<ArrowLeft class="size-4" />
		</Button>
		<div class="min-w-0">
			<h1 class="truncate text-sm font-semibold">{t("mods.title")}</h1>
			<p class="truncate text-xs text-muted-foreground">
				{launcher.browsing?.name}{#if address}<span class="font-mono text-muted-foreground/70"
					>&nbsp;·&nbsp;{address}</span
				>{/if}
			</p>
		</div>
	</div>

	<!-- One control, not three buttons: the track carries the group, the thumb the choice. -->
	<div class="px-6 pt-3">
		<div class="inline-flex rounded-lg border border-border/70 bg-muted/40 p-0.5">
			{#each tabs as tab (tab.id)}
				{@const active = launcher.tab === tab.id}
				<button
					type="button"
					aria-pressed={active}
					onclick={() => switchTab(tab.id)}
					class={cn(
						"h-7 rounded-md px-3 text-xs font-medium transition-colors duration-150 outline-none",
						"focus-visible:ring-3 focus-visible:ring-ring/50",
						active
							? "bg-secondary text-foreground"
							: "text-muted-foreground hover:text-foreground"
					)}
				>
					{t(tab.key)}
				</button>
			{/each}
		</div>
	</div>

	<!-- clamped, not truncated: the second sentence is the one that explains a thin catalogue -->
	<p class="line-clamp-2 px-6 pt-2.5 text-xs text-muted-foreground" title={hint}>{hint}</p>

	{#if launcher.tab === "search"}
		<div class="px-6 pt-3">
			<Input
				bind:value={query}
				oninput={() => search(query)}
				placeholder={t("mods.search")}
				aria-label={t("mods.search")}
			/>
		</div>
	{/if}

	<!--
		The 10px scrollbar comes out of this pane's own right padding rather than being added to
		it, and the gutter is held whether the list overflows or not: otherwise every row sits ten
		pixels left of the search field above them the moment there is more than a screenful.
	-->
	<div class="mt-3 min-h-0 flex-1 overflow-y-auto pl-6 pr-[calc(1.5rem_-_10px)] [scrollbar-gutter:stable]">
		<div class="flex flex-col gap-1.5 pb-4">
			{#each launcher.hits as hit (hit.id)}
				{@const chosen = launcher.picked.has(hit.id)}
				<button
					type="button"
					disabled={!selectable(hit)}
					onclick={() => launcher.toggle(hit.id)}
					class="flex items-center gap-3 rounded-lg border px-3.5 py-2.5 text-left transition-colors
					       duration-150 outline-none focus-visible:ring-3 focus-visible:ring-ring/50
					       disabled:cursor-default
					       {chosen
						? 'border-primary/60 bg-primary/15'
						: hit.on_server || hit.installed
							? 'border-border/50 bg-muted/40'
							: 'border-border/70 bg-card hover:border-border hover:bg-accent/40'}"
				>
					<div
						class="flex size-11 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted"
					>
						{#if hit.icon && !broken.has(hit.id)}
							<!-- pixel art, mostly: smoothing it on the way down makes it mush -->
							<img
								src={hit.icon}
								alt=""
								loading="lazy"
								onerror={() => (broken = new Set(broken).add(hit.id))}
								class="size-11 object-cover [image-rendering:auto]"
							/>
						{:else}
							<Package class="size-4 text-muted-foreground/60" />
						{/if}
					</div>

					<div class="min-w-0 flex-1">
						<p
							class="truncate text-sm {hit.on_server || hit.installed
								? 'text-muted-foreground'
								: ''}"
							title={hit.title}
						>
							{hit.title}
						</p>
						<!-- Always drawn: a mod without a description may not make its row shorter. -->
						<p class="min-h-4 truncate text-xs text-muted-foreground/70" title={hit.description}>
							{hit.description}
						</p>
					</div>

					{#if hit.on_server}
						<span class="flex shrink-0 items-center gap-1.5 text-xs text-ok">
							<Check class="size-4" />
							{t("mods.inPack")}
						</span>
					{:else if hit.installed}
						<span class="flex shrink-0 items-center gap-1.5 text-xs text-ok">
							<Check class="size-4" />
							{t("mods.installed")}
						</span>
					{:else if launcher.tab === "installed"}
						<span class="shrink-0 text-xs text-muted-foreground">
							{chosen ? t("mods.willBeRemoved") : ""}
						</span>
					{:else}
						<span class="shrink-0 font-mono text-xs text-muted-foreground/70">
							{compact(hit.downloads)}
							{hit.source === "modrinth" ? "MR" : "CF"}
						</span>
					{/if}
				</button>
			{/each}

			<!--
				The first page of a tab has nothing to show yet. Saying so beats an empty panel
				that reads as "no results" until the network answers.
			-->
			{#if launcher.hits.length === 0}
				<p class="py-10 text-center text-sm text-muted-foreground">
					{#if launcher.loadingMods}
						{t("common.loading")}
					{:else}
						{launcher.note || t("mods.nothing")}
					{/if}
				</p>
			{/if}

			{#if launcher.more}
				<!-- The label stays put while it loads; only the spinner appears, so the row never moves. -->
				<Button
					variant="ghost"
					class="mt-1 w-full gap-2 text-muted-foreground"
					disabled={launcher.loadingMods}
					onclick={() => launcher.loadMore()}
				>
					{#if launcher.loadingMods}
						<Loader2 class="size-4 animate-spin" />
					{/if}
					{t("common.more")}
				</Button>
			{/if}
		</div>
	</div>

	<!-- Fixed height: the bar carries three different states and may not resize between them. -->
	<div class="flex h-14 shrink-0 items-center gap-3 border-t border-border/60 px-6">
		<span
			class="flex h-5 min-w-0 flex-1 items-center gap-2 truncate text-xs {launcher.note
				? 'text-foreground'
				: 'text-muted-foreground'}"
		>
			{#if launcher.loadingMods}
				<Loader2 class="size-4 shrink-0 animate-spin" />
				{t("common.loading")}
			{:else if launcher.picked.size > 0}
				{t("mods.selected", { count: launcher.picked.size })}
			{:else}
				<span class="truncate" title={launcher.note}>{launcher.note}</span>
			{/if}
		</span>

		{#if launcher.tab !== "pack"}
			<Button
				class="shrink-0"
				variant={launcher.tab === "installed" ? "destructive" : "default"}
				disabled={launcher.picked.size === 0 || launcher.loadingMods}
				onclick={() => launcher.applyMods()}
			>
				{#if launcher.tab === "installed"}
					<Trash2 class="size-4" />
					{t("mods.remove")}
				{:else}
					<Download class="size-4" />
					{t("mods.install")}
				{/if}
			</Button>
		{/if}
	</div>
</div>
