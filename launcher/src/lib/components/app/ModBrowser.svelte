<script lang="ts">
	import {
		ArrowLeft,
		Check,
		Download,
		ExternalLink,
		Loader2,
		Lock,
		Package,
		Search,
		Trash2
	} from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import * as Select from "$lib/components/ui/select";
	import { call } from "$lib/bridge";
	import { t } from "$lib/i18n.svelte";
	import { KINDS, SORTS, launcher, type Kind, type Sort, type Tab } from "$lib/state.svelte";
	import type { Hit } from "$lib/types";
	import { cn } from "$lib/utils";

	let query = $state("");
	let timer: ReturnType<typeof setTimeout>;

	/** Icons come from two catalogues; a dead URL must not leave a broken-image glyph behind. */
	let broken = $state<Set<string>>(new Set());

	const tabs: { id: Tab; key: string }[] = [
		{ id: "search", key: "mods.tab.search" },
		{ id: "installed", key: "mods.tab.here" }
	];

	/** An instance or a server run here has no server on the other side to conflict with. */
	const own = $derived(
		Boolean(
			launcher.browsing?.address.startsWith("instance-") ||
				launcher.browsing?.address.startsWith("host-")
		)
	);

	/**
	 * Why a category is missing. Two reasons, and they are not the same sentence: a pack
	 * decides what may be added to it, and a setup with no loader cannot run a mod at all.
	 */
	const restricted = $derived.by(() => {
		if (launcher.allowedKinds.length >= KINDS.length) return "";
		if (!launcher.allowedKinds.includes("mod")) return t("mods.noLoader");
		return t("mods.restricted", {
			kinds: launcher.allowedKinds.map((kind) => t(`mods.kind.${kind}`)).join(", ")
		});
	});

	/** An instance is browsed through a synthetic address nobody should have to read. */
	const address = $derived(
		launcher.browsing && !own ? launcher.browsing.address : ""
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

	function switchKind(kind: Kind) {
		if (kind === launcher.kind) return;
		clearTimeout(timer);
		query = "";
		void launcher.switchKind(kind);
	}

	function compact(count: number) {
		if (count >= 1_000_000) return `${Math.round(count / 1_000_000)}M`;
		if (count >= 1_000) return `${Math.round(count / 1_000)}k`;
		return String(count);
	}

	/** The installed list is a folder, not a catalogue, so it narrows here rather than remotely. */
	const rows = $derived.by(() => {
		if (launcher.tab !== "installed") return launcher.hits;
		const needle = launcher.installedFilter.trim().toLowerCase();
		if (!needle) return launcher.hits;
		return launcher.hits.filter(
			(hit) =>
				hit.title.toLowerCase().includes(needle) || hit.author.toLowerCase().includes(needle)
		);
	});

	/** Where a row came from, in a word. "MR" and "CF" were initials nobody was taught. */
	function origin(hit: Hit): string {
		if (hit.source === "modrinth") return "Modrinth";
		if (hit.source === "curseforge") return "CurseForge";
		return t(hit.source === "pack" ? "mods.fromServer" : "mods.fromYou");
	}

	/** The project's own page, in the player's browser. Only ever an http(s) catalogue link. */
	function openPage(hit: Hit) {
		if (/^https:\/\//.test(hit.url)) void call("open_catalogue_page", { url: hit.url });
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

	<div class="flex items-center gap-3 px-6 pt-3">
		<!-- One control, not two buttons: the track carries the group, the thumb the choice. -->
		<div class="inline-flex shrink-0 rounded-lg border border-border/70 bg-muted/40 p-0.5">
			{#each tabs as tab (tab.id)}
				{@const active = launcher.tab === tab.id}
				<button
					type="button"
					aria-pressed={active}
					onclick={() => switchTab(tab.id)}
					class={cn(
						"h-7 rounded-md px-3 text-xs font-medium whitespace-nowrap transition-colors duration-150 outline-none",
						"focus-visible:ring-3 focus-visible:ring-ring/50",
						active ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"
					)}
				>
					{t(tab.key)}
				</button>
			{/each}
		</div>

		<div class="flex-1"></div>

		<!-- Ordering is the catalogue's business, so it is offered only where there is one. -->
		{#if launcher.tab === "search"}
			<Select.Root
				type="single"
				value={launcher.sort}
				onValueChange={(value) => void launcher.setSort(value as Sort)}
			>
				<Select.Trigger class="h-8 w-36 text-xs" aria-label={t("mods.sortLabel")}>
					<span>{t(`mods.sort.${launcher.sort}`)}</span>
				</Select.Trigger>
				<Select.Content>
					{#each SORTS as option (option)}
						<Select.Item value={option} label={t(`mods.sort.${option}`)} />
					{/each}
				</Select.Content>
			</Select.Root>
		{/if}

		<!-- Nothing to choose between when the server allows exactly one category. -->
		{#if launcher.allowedKinds.length > 1}
			<Select.Root
				type="single"
				value={launcher.kind}
				onValueChange={(value) => switchKind(value as Kind)}
			>
				<Select.Trigger class="h-8 w-40 text-xs" aria-label={t("mods.kindLabel")}>
					<span>{t(`mods.kind.${launcher.kind}`)}</span>
				</Select.Trigger>
				<Select.Content>
					{#each launcher.allowedKinds as kind (kind)}
						<Select.Item value={kind} label={t(`mods.kind.${kind}`)} />
					{/each}
				</Select.Content>
			</Select.Root>
		{/if}
	</div>

	{#if restricted}
		<p class="line-clamp-2 px-6 pt-2 text-xs text-warn" title={restricted}>{restricted}</p>
	{/if}

	<!-- One box, both tabs: it searches the catalogue on one and narrows the folder on the other. -->
	<div class="relative px-6 pt-3">
		<Search
			class="pointer-events-none absolute top-1/2 left-8.5 size-3.5 -translate-y-1/2 text-muted-foreground"
		/>
		{#if launcher.tab === "search"}
			<Input
				bind:value={query}
				oninput={() => search(query)}
				placeholder={own ? t("mods.searchOwn") : t("mods.search")}
				aria-label={t("mods.search")}
				class="pl-8"
			/>
		{:else}
			<Input
				bind:value={launcher.installedFilter}
				placeholder={t("mods.filterHere")}
				aria-label={t("mods.filterHere")}
				class="pl-8"
			/>
		{/if}
	</div>

	<!--
		The 10px scrollbar comes out of this pane's own right padding rather than being added to
		it, and the gutter is held whether the list overflows or not: otherwise every row sits ten
		pixels left of the search field above them the moment there is more than a screenful.
	-->
	<div
		class="mt-3 min-h-0 flex-1 overflow-y-auto pr-[calc(1.5rem_-_10px)] pl-6 [scrollbar-gutter:stable]"
	>
		<div class="flex flex-col gap-1.5 pb-4">
			{#each rows as hit (hit.id)}
				{@const busy = launcher.working === hit.id}
				{@const had = hit.on_server || hit.installed}
				<article
					class="surface flex items-center gap-3 px-3.5 py-2.5 transition-colors duration-150
					       {had ? 'opacity-80' : 'surface-hover'}"
				>
					<div
						class="flex size-11 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted"
					>
						{#if hit.icon && !broken.has(hit.id)}
							<img
								src={hit.icon}
								alt=""
								loading="lazy"
								onerror={() => (broken = new Set(broken).add(hit.id))}
								class="size-11 object-cover"
							/>
						{:else}
							<Package class="size-4 text-muted-foreground/60" />
						{/if}
					</div>

					<div class="min-w-0 flex-1">
						<div class="flex items-baseline gap-2">
							<p class="truncate text-sm font-medium" title={hit.title}>{hit.title}</p>
							{#if hit.on_server}
								<span class="flex shrink-0 items-center gap-1 text-xs text-ok">
									<Check class="size-3" />
									{t("mods.inPack")}
								</span>
							{:else if hit.installed}
								<span class="flex shrink-0 items-center gap-1 text-xs text-ok">
									<Check class="size-3" />
									{t("mods.installed")}
								</span>
							{/if}
						</div>

						<!--
							Who wrote it, which build, and where it came from - the three things worth
							knowing before installing something, and none of them were shown before.
						-->
						<div class="mt-0.5 flex flex-wrap items-center gap-1">
							{#if hit.author}
								<span class="chip">{t("mods.by", { author: hit.author })}</span>
							{/if}
							{#if hit.version}
								<span class="chip max-w-56 truncate" title={hit.version}>{hit.version}</span>
							{/if}
							<!--
								Only for a catalogue row. On a local one the control at the end of the
								row already says where it came from, and saying it twice is noise.
							-->
							{#if hit.source === "modrinth" || hit.source === "curseforge"}
								<span class="chip">{origin(hit)}</span>
							{/if}
							{#if hit.downloads > 0}
								<span class="chip">{t("mods.downloads", { count: compact(hit.downloads) })}</span>
							{/if}
						</div>

						{#if hit.description}
							<p class="mt-0.5 truncate text-xs text-muted-foreground/70" title={hit.description}>
								{hit.description}
							</p>
						{/if}
					</div>

					<div class="flex shrink-0 items-center gap-1">
						{#if hit.url}
							<Button
								variant="ghost"
								size="icon"
								class="text-muted-foreground hover:text-foreground"
								onclick={() => openPage(hit)}
								title={t("mods.openPage")}
								aria-label={t("mods.openPageOf", { name: hit.title })}
							>
								<ExternalLink class="size-4" />
							</Button>
						{:else}
							<div class="size-8" aria-hidden="true"></div>
						{/if}

						<!--
							One button per row, saying exactly what it does to that row. The old
							screen had one highlight meaning "will be installed" on one tab and
							"will be deleted" on another, told apart only by a button elsewhere.
						-->
						{#if hit.on_server}
							<span
								class="flex h-8 w-28 shrink-0 items-center justify-center gap-1.5 text-xs
								       whitespace-nowrap text-muted-foreground"
								title={t("mods.serverDecides")}
							>
								<Lock class="size-3" />
								{t("mods.fromServer")}
							</span>
						{:else if hit.removable}
							<Button
								variant="ghost"
								class="w-28 text-muted-foreground hover:bg-destructive/15 hover:text-destructive"
								disabled={busy || Boolean(launcher.working)}
								onclick={() => launcher.uninstall(hit)}
							>
								{#if busy}
									<Loader2 class="size-4 animate-spin" />
								{:else}
									<Trash2 class="size-4" />
									{t("mods.remove")}
								{/if}
							</Button>
						{:else if hit.installed}
							<span class="flex h-8 w-28 items-center justify-center text-xs text-ok">
								<Check class="size-4" />
							</span>
						{:else}
							<Button
								class="w-28"
								disabled={busy || Boolean(launcher.working)}
								onclick={() => launcher.install(hit)}
							>
								{#if busy}
									<Loader2 class="size-4 animate-spin" />
								{:else}
									<Download class="size-4" />
									{t("mods.install")}
								{/if}
							</Button>
						{/if}
					</div>
				</article>
			{/each}

			<!--
				The first page of a tab has nothing to show yet. Saying so beats an empty panel
				that reads as "no results" until the network answers.
			-->
			{#if rows.length === 0}
				<p class="py-10 text-center text-sm text-muted-foreground">
					{#if launcher.loadingMods}
						{t("common.loading")}
					{:else if launcher.tab === "installed" && launcher.installedFilter}
						{t("common.noMatches", { query: launcher.installedFilter })}
					{:else}
						{launcher.note || t("mods.nothing")}
					{/if}
				</p>
			{/if}

			{#if launcher.more && launcher.tab === "search"}
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

	<!--
		What just happened, and nothing else. The bar used to carry the only control that
		committed anything; now every row commits its own, so this is a status line.
	-->
	{#if launcher.note || launcher.loadingMods}
		<div class="flex h-11 shrink-0 items-center gap-2 border-t border-border/60 px-6">
			{#if launcher.loadingMods}
				<Loader2 class="size-4 shrink-0 animate-spin text-muted-foreground" />
				<span class="text-xs text-muted-foreground">{t("common.loading")}</span>
			{:else}
				<span class="min-w-0 truncate text-xs" title={launcher.note}>{launcher.note}</span>
			{/if}
		</div>
	{/if}
</div>
