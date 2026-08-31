<script lang="ts">
	import {
		Copy,
		FolderOpen,
		Loader2,
		Package,
		Play,
		Radio,
		Server,
		Square,
		Terminal,
		Trash2,
		LogIn,
		Users,
		Zap
	} from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";
	import type { HostedServer } from "$lib/types";

	let { server }: { server: HostedServer } = $props();

	const busy = $derived(launcher.switching === server.id);
	const watching = $derived(launcher.watching === server.id);

	/** Copied for a moment, so pressing it is visibly acknowledged. */
	let copied = $state(false);

	/** Already in the server list, so the button that puts it there has nothing left to do. */
	const known = $derived(launcher.servers.some((entry) => entry.address === server.address));

	async function join() {
		await launcher.add(server.name, server.address);
		launcher.show("servers");
	}

	async function copy() {
		try {
			await navigator.clipboard.writeText(server.address);
			copied = true;
			setTimeout(() => (copied = false), 1400);
		} catch {
			// A webview without clipboard permission; the address is on screen to read anyway.
		}
	}
</script>

<!--
	The left edge carries the state, not a dot in the corner: a list of servers is scanned down
	the left margin, and a running one has to be findable without reading a word of it.
-->
<article class="surface surface-hover group">
	<span
		aria-hidden="true"
		class="absolute inset-y-0 left-0 w-[3px] transition-colors duration-300 {server.running
			? 'bg-ok'
			: 'bg-border'}"
	></span>

	<div class="flex items-center gap-3.5 py-3 pr-4 pl-5">
		<div
			class="relative flex size-11 shrink-0 items-center justify-center rounded-lg bg-muted/70"
		>
			<Server class="size-5 text-muted-foreground-dim" />
			{#if server.running}
				<!-- A running server has a heartbeat; a stopped one is a plain box. -->
				<span class="absolute -top-1 -right-1 flex size-3">
					<span class="absolute inline-flex size-full animate-ping rounded-full bg-ok opacity-70"
					></span>
					<span class="relative inline-flex size-3 rounded-full bg-ok"></span>
				</span>
			{/if}
		</div>

		<div class="min-w-0 flex-1">
			<div class="flex items-baseline gap-2">
				<h3 class="truncate text-base font-semibold" title={server.name}>{server.name}</h3>
				{#if server.running && server.players.length > 0}
					<span
						class="flex shrink-0 items-center gap-1 text-xs text-ok"
						title={server.players.join(", ")}
					>
						<Users class="size-3" />
						{server.players.length}
					</span>
				{/if}
			</div>

			<!-- The facts as pills: version, loader, heap, and where it came from. -->
			<div class="mt-1 flex flex-wrap items-center gap-1">
				<span class="chip chip-strong">{server.minecraft}</span>
				{#if server.loader}
					<span class="chip">{server.loader}</span>
				{:else}
					<span class="chip">{t("host.vanilla")}</span>
				{/if}
				<span class="chip">{server.memory} MB</span>
				{#if server.publishes}
					<!--
						The one thing no other launcher's server does: a player who arrives with an
						empty mods folder still gets in, because the server hands them the pack.
					-->
					<span class="chip chip-strong text-ok" title={t("host.publishesHint")}>
						<Zap class="size-2.5" />
						{t("host.publishes")}
					</span>
				{/if}
				{#if server.source}
					<span class="chip max-w-52 truncate" title={server.source}>{server.source}</span>
				{/if}
			</div>

			<!-- The address is the point of the whole card: it is what a friend has to be sent. -->
			<button
				type="button"
				onclick={copy}
				title={t("host.copy")}
				class="mt-1.5 -ml-1 flex items-center gap-1.5 rounded px-1 py-0.5 font-mono text-xs
				       text-muted-foreground-dim outline-none transition-colors hover:bg-accent/40
				       hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
			>
				{server.address}
				{#if copied}
					<span class="text-ok">{t("host.copied")}</span>
				{:else}
					<Copy class="size-3 opacity-0 transition-opacity group-hover:opacity-60" />
				{/if}
			</button>
		</div>

		<div class="flex shrink-0 items-center gap-1">
			<Button
				variant="ghost"
				size="icon"
				class="text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100
				       hover:text-destructive focus-visible:opacity-100"
				disabled={server.running}
				onclick={() => launcher.removeHost(server.id, true)}
				title={server.running ? t("host.stopItFirst") : t("host.remove")}
				aria-label={t("host.action.remove", { name: server.name })}
			>
				<Trash2 class="size-4" />
			</Button>

			<!--
				The loop closed: a server you just built is one you want to be on, and its
				address belongs in the list the play button reads from. Only while it is up -
				adding an address nothing answers on would just be a red row.
			-->
			{#if server.running && !known}
				<Button
					variant="ghost"
					size="icon"
					class="text-muted-foreground hover:text-foreground"
					onclick={join}
					title={t("host.join")}
					aria-label={t("host.action.join", { name: server.name })}
				>
					<LogIn class="size-4" />
				</Button>
			{:else}
				<!-- The slot stays: without it every other icon in the row slides one place, and
				     two neighbouring cards stop sharing a vertical line. -->
				<div class="size-8 shrink-0" aria-hidden="true"></div>
			{/if}

			<Button
				variant="ghost"
				size="icon"
				class="text-muted-foreground hover:text-foreground"
				onclick={() => launcher.openHostDir(server.id)}
				title={t("host.openFolder")}
				aria-label={t("host.action.folder", { name: server.name })}
			>
				<FolderOpen class="size-4" />
			</Button>

			<Button
				variant="ghost"
				size="icon"
				class="text-muted-foreground hover:text-foreground"
				onclick={() => launcher.openHostMods(server)}
				title={t("host.content")}
				aria-label={t("host.action.content", { name: server.name })}
			>
				<Package class="size-4" />
			</Button>

			<Button
				variant="ghost"
				size="icon"
				class={watching ? "text-foreground" : "text-muted-foreground hover:text-foreground"}
				onclick={() => (watching ? launcher.unwatch() : launcher.watch(server.id))}
				title={t("host.console")}
				aria-label={t("host.action.console", { name: server.name })}
			>
				<Terminal class="size-4" />
			</Button>

			{#if server.running}
				<Button
					size="lg"
					variant="secondary"
					class="min-w-28"
					disabled={busy}
					onclick={() => launcher.stopHost(server.id)}
				>
					{#if busy}
						<Loader2 class="size-4 animate-spin" />
						{t("host.stopping")}
					{:else}
						<Square class="size-3.5 fill-current" />
						{t("host.stop")}
					{/if}
				</Button>
			{:else}
				<Button
					size="lg"
					class="cta-glow bg-cta text-cta-foreground hover:bg-cta/90 min-w-28"
					disabled={busy}
					onclick={() => launcher.startHost(server.id)}
				>
					{#if busy}
						<Loader2 class="size-4 animate-spin" />
						{t("host.starting")}
					{:else}
						<Play class="size-4 fill-current" />
						{t("host.start")}
					{/if}
				</Button>
			{/if}
		</div>
	</div>

	<!--
		Who is on, under the row rather than in a tooltip: on a server you host, "is anyone
		playing" is the question you opened the tab to answer.
	-->
	{#if server.running && server.players.length > 0}
		<div
			class="flex flex-wrap items-center gap-1.5 border-t border-border/50 bg-background/30 px-5 py-2"
		>
			<Radio class="size-3 shrink-0 text-ok" />
			{#each server.players as player (player)}
				<span class="chip chip-strong">{player}</span>
			{/each}
		</div>
	{/if}
</article>
