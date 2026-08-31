<script lang="ts">
	import { Clock, Cog, Gamepad2, Loader2, Package, Play, Server, Trash2 } from "@lucide/svelte";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";
	import { ago, spell } from "$lib/played";
	import Motd from "./Motd.svelte";
	import { Button } from "$lib/components/ui/button";
	import StatusDot from "./StatusDot.svelte";
	import type { SavedServer, ServerStatus } from "$lib/types";
	import type { ServerState } from "./StatusDot.svelte";

	let {
		server,
		status,
		busy = false,
		onplay,
		onmods,
		onsetup,
		onremove
	}: {
		server: SavedServer;
		status?: ServerStatus;
		busy?: boolean;
		onplay: () => void;
		onmods: () => void;
		onsetup: () => void;
		onremove: () => void;
	} = $props();

	/** Filed under the address, which is what survives a row being removed and added back. */
	const played = $derived(launcher.played(`server:${server.address}`));

	const state = $derived<ServerState>(
		!status ? "checking" : !status.online ? "offline" : !status.manifest ? "unknown" : "ready"
	);

	const manifest = $derived(status?.manifest ?? null);

	const singleplayer = $derived(server.address.trim() === "");

	/**
	 * What the row says about itself, as pills rather than a dot-separated sentence: version,
	 * loader, and how much it will pull. A list is scanned, not read.
	 *
	 * While there is nothing to say pills yet, one sentence takes their place - "checking" is
	 * a state, not a fact about the server.
	 */
	const chips = $derived.by(() => {
		if (!manifest) return [];
		const parts = [{ text: manifest.minecraft, strong: true }];
		if (manifest.loader) parts.push({ text: manifest.loader.type, strong: false });
		if (manifest.mods.length)
			parts.push({ text: t("servers.mods", { count: manifest.mods.length }), strong: false });
		if (manifest.config.length)
			parts.push({ text: t("servers.configs", { count: manifest.config.length }), strong: false });
		return parts;
	});

	/** The one line shown when there is nothing to draw pills from. */
	const note = $derived(
		!status
			? t("servers.facts.checking")
			: // The row says it in the player's language; what the socket actually said is English
				// prose from a library, and it belongs in the tooltip rather than in the list.
				!status.online
				? t("servers.facts.unreachable")
				: !manifest
					? t("servers.facts.noManifest")
					: ""
	);

	/** Reachable and with a version to install. A server that publishes no pack is normal. */
	const canPlay = $derived(Boolean(status?.online && manifest));

	/** No other row may start while one is starting; the overlay covers the list either way. */
	const blocked = $derived(Boolean(launcher.playing) && !busy);

	/** Already running: the row says so rather than offering a start that is refused. */
	const live = $derived(Boolean(launcher.running[server.id]));

	/**
	 * A pack states its own version and loader, so there is nothing to override. The setup dialog
	 * is for servers that announce neither.
	 */
	const prescribed = $derived((manifest?.mods.length ?? 0) > 0);
</script>

<article class="surface surface-hover group flex items-center gap-3.5 py-3 pr-4 pl-5">
	<!--
		The state on the left edge, not as a dot in the row: a list is scanned down its left
		margin, and one offline server among twenty has to be findable without reading.
	-->
	<span
		aria-hidden="true"
		class="absolute inset-y-0 left-0 w-[3px] transition-colors duration-300 {state === 'ready'
			? 'bg-ok'
			: state === 'unknown'
				? 'bg-warn'
				: state === 'offline'
					? 'bg-bad'
					: 'bg-border'}"
	></span>

	<!--
		Visible rather than sr-only: the stripe carries the state in colour and nothing else, and
		the glyph is what an eye that cannot split the green from the red actually reads. Same slot,
		same size, every state - a ready row and an unreachable one keep one geometry.
	-->
	<StatusDot {state} />

	<!-- The server's own icon if it has one; a placeholder keeps the row aligned if not. -->
	<!-- square, not rounded: server icons are pixel art and their own corners are the point -->
	<div class="flex size-11 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-muted/70">
		<!--
			The server's own icon, or the one Minecraft shows for a server without one. That
			texture comes from the client jar on this machine rather than shipped with us -
			it is Mojang's, and handing it out of a public repository would be redistributing
			a game file.
		-->
		{#if status?.icon ?? launcher.unknownServerIcon}
			<img
				src={status?.icon ?? launcher.unknownServerIcon}
				alt=""
				class="size-full object-cover [image-rendering:pixelated]"
			/>
		{:else}
			<Server class="size-4 text-muted-foreground-dim" />
		{/if}
	</div>

	<div class="min-w-0 flex-1">
		<div class="flex items-baseline gap-2">
			<h3 class="truncate text-base font-semibold" title={server.name}>{server.name}</h3>
			{#if status?.online}
				<span class="shrink-0 font-mono text-xs text-muted-foreground">
					{t("servers.players", { online: status.players, max: status.max_players })}
				</span>
			{/if}
		</div>

		<!--
			Description and address share one line that is always drawn, empty or not: a server
			without a description has to leave a row exactly as tall as the one under it, or the
			play buttons of the list stop sitting on a single line.
		-->
		<div class="flex min-h-4 items-center gap-2">
			<div class="min-w-0 flex-1 text-xs text-muted-foreground">
				{#if status?.motd?.length}
					<Motd spans={status.motd} />
				{/if}
			</div>
			{#if !singleplayer}
				<!-- capped, not just shrink-0: a 200-character hostname may not push the row wider -->
				<span
					class="max-w-[55%] shrink-0 truncate font-mono text-xs text-muted-foreground-dim"
					title={server.address}
				>
					{server.address}
				</span>
			{/if}
		</div>

		<div class="mt-1 flex min-h-4 flex-wrap items-center gap-1">
			{#each chips as chip, i (i)}
				<span class="chip {chip.strong ? 'chip-strong' : ''}">{chip.text}</span>
			{/each}
			{#if note}
				<span class="truncate text-xs text-muted-foreground" title={status?.error ?? note}>
					{note}
				</span>
			{/if}

			<!--
				Playtime last in the row: it is the one fact about the player rather than about
				the server, so it reads as a note on the end rather than another property of it.
			-->
			{#if played && played.seconds > 0}
				<span class="flex shrink-0 items-center gap-1 text-[0.7rem] text-muted-foreground-dim">
					<Clock class="size-2.5" />
					{spell(played.seconds)}
					<span class="text-muted-foreground-dim">·</span>
					{ago(played.last)}
				</span>
			{/if}
		</div>
	</div>

	<!--
		Fixed width, every slot always occupied: the play buttons of neighbouring rows have to sit
		on one vertical line, and a row that happens to have no mods button must not pull its own
		play button rightwards.
	-->
	<div class="flex shrink-0 items-center gap-1">
		<Button
			variant="ghost"
			size="icon"
			class="text-muted-foreground opacity-0 transition-opacity
			       group-hover:opacity-100 hover:text-destructive focus-visible:opacity-100"
			onclick={onremove}
			aria-label={t("servers.action.remove", { name: server.name })}
		>
			<Trash2 class="size-4" />
		</Button>

		{#if !prescribed}
			<Button
				variant="ghost"
				size="icon"
				class="text-muted-foreground hover:text-foreground"
				onclick={onsetup}
				title={t("servers.setup")}
				aria-label={t("servers.action.setup", { name: server.name })}
			>
				<Cog class="size-4" />
			</Button>
		{:else}
			<!-- The gap stays, so a pack row and a vanilla row keep the same action geometry. -->
			<div class="size-8 shrink-0" aria-hidden="true"></div>
		{/if}

		<!--
			Wherever there is a manifest to install against, loader or not: resource packs, shaders
			and datapacks need no loader at all, and a server that announces none is exactly the
			one that leaves all four categories to the player. Only an unreachable server has
			nothing to browse - the gap it leaves stays reserved so the row keeps its width.
		-->
		{#if canPlay}
			<Button
				variant="ghost"
				size="icon"
				class="text-muted-foreground hover:text-foreground"
				onclick={onmods}
				title={t("mods.title")}
				aria-label={t("servers.action.mods", { name: server.name })}
			>
				<Package class="size-4" />
			</Button>
		{:else}
			<div class="size-8" aria-hidden="true"></div>
		{/if}

		<Button
			size="lg"
			class="cta-glow bg-cta text-cta-foreground hover:bg-cta/90 min-w-28"
			onclick={onplay}
			disabled={busy || blocked || !canPlay}
		>
			{#if busy}
				<Loader2 class="size-4 animate-spin" />
				{t("common.starting")}
			{:else if live}
				<!-- Running: a second start would only be refused, and the row should say why -->
				<Gamepad2 class="size-4" />
				{t("common.playing")}
			{:else}
				<Play class="size-4 fill-current" />
				{t("common.play")}
			{/if}
		</Button>
	</div>
</article>
