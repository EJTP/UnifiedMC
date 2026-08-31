<script lang="ts">
	import { Boxes, Loader2, LogIn, Server, Settings, Radio, HardDrive } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import SkinDialog from "./SkinDialog.svelte";
	import SignInDialog from "./SignInDialog.svelte";
	import { t } from "$lib/i18n.svelte";
	import { spell } from "$lib/played";
	import { launcher, type View } from "$lib/state.svelte";
	import Sparkline from "./Sparkline.svelte";
	import PlayerHead from "./PlayerHead.svelte";
	import { cn } from "$lib/utils";

	let { onsettings }: { onsettings: () => void } = $props();

	/** The account block owns the skin, so the dialog lives here rather than in the page. */
	let skinOpen = $state(false);

	const items: { id: View; key: string; icon: typeof Server; count: () => number }[] = [
		{ id: "servers", key: "nav.servers", icon: Server, count: () => launcher.servers.length },
		{ id: "instances", key: "nav.instances", icon: Boxes, count: () => launcher.instances.length },
		{ id: "hosting", key: "nav.hosting", icon: HardDrive, count: () => launcher.hosts.length }
	];

	/** Servers running on this machine right now. The sidebar is where you notice one is up. */
	const live = $derived(launcher.hosts.filter((server) => server.running));

	/**
	 * Every day everything was played, added together - the trend is about the player, not
	 * about one row. Two weeks, because that is what a sidebar's width can draw honestly.
	 */
	const SPAN = 14;

	const everyDay = $derived.by(() => {
		const total: Record<number, number> = {};
		for (const played of Object.values(launcher.playtime)) {
			for (const [day, seconds] of Object.entries(played.days)) {
				total[Number(day)] = (total[Number(day)] ?? 0) + seconds;
			}
		}
		return total;
	});

	const fortnight = $derived(
		Array.from({ length: SPAN }, (_, i) => everyDay[launcher.today - i] ?? 0).reduce(
			(sum, seconds) => sum + seconds,
			0
		)
	);
</script>

<!--
	One inset all the way down - px-2 outside, px-2 inside every row - so the nav icons, the
	counts and the account line up on the same two edges instead of three rhythms.
-->
<nav class="flex w-52 shrink-0 flex-col border-r border-border/60 bg-card/30">
	<div class="flex flex-col gap-0.5 px-2 pt-3">
		{#each items as item (item.id)}
			{@const active = launcher.view === item.id}
			<button
				type="button"
				aria-current={active ? "page" : undefined}
				onclick={() => launcher.show(item.id)}
				class={cn(
					"relative flex items-center gap-2.5 rounded-md px-2 py-2 text-sm outline-none",
					"transition-colors duration-150 focus-visible:ring-3 focus-visible:ring-ring/50",
					active
						? "bg-secondary text-foreground"
						: "text-muted-foreground hover:bg-accent/40 hover:text-foreground"
				)}
			>
				<!--
					The marker is on the left edge rather than the whole row being tinted: it
					survives the hover fill, so hovering an inactive row cannot look like
					selecting it.
				-->
				{#if active}
					<span
						aria-hidden="true"
						class="absolute inset-y-1.5 -left-2 w-[3px] rounded-r-full bg-primary"
					></span>
				{/if}
				<item.icon class="size-4 shrink-0" />
				<span class="flex-1 truncate text-left">{t(item.key)}</span>
				{#if item.count() > 0}
					<span class="shrink-0 font-mono text-xs text-muted-foreground-dim">{item.count()}</span>
				{/if}
			</button>
		{/each}
	</div>

	<!--
		What is up right now, listed rather than counted: a server left running is the thing
		this window has to keep visible, because nothing else on the desktop shows it.
	-->
	{#if live.length > 0}
		<div class="mt-5 px-2">
			<p class="flex items-center gap-1.5 px-2 pb-1.5 text-[0.7rem] tracking-wide text-ok uppercase">
				<Radio class="size-3" />
				{t("nav.running")}
			</p>
			{#each live as server (server.id)}
				<button
					type="button"
					onclick={() => {
						launcher.show("hosting");
						void launcher.watch(server.id);
					}}
					class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left outline-none
					       transition-colors hover:bg-accent/40 focus-visible:ring-3 focus-visible:ring-ring/50"
				>
					<span class="size-1.5 shrink-0 rounded-full bg-ok"></span>
					<span class="min-w-0 flex-1 truncate text-xs text-muted-foreground">{server.name}</span>
					{#if server.players.length > 0}
						<span class="shrink-0 font-mono text-[0.7rem] text-ok">{server.players.length}</span>
					{/if}
				</button>
			{/each}
		</div>
	{/if}

	<div class="flex-1"></div>

	<!--
		A stat tile, not a chart: the number is the answer and the trend is the shape behind
		it. Drawn only once there is something to draw - an empty fortnight of bars is a
		decoration that says nothing.
	-->
	{#if fortnight > 0}
		<div class="px-2 pb-3">
			<div class="rounded-lg bg-card/50 px-2.5 py-2">
				<div class="flex items-baseline justify-between gap-2">
					<span class="text-[0.7rem] tracking-wide text-muted-foreground-dim uppercase">
						{t("played.title")}
					</span>
					<!-- Text wears text tokens; the bars beside it carry the colour. -->
					<span class="font-mono text-sm text-foreground">{spell(fortnight)}</span>
				</div>
				<div class="mt-1.5">
					<Sparkline days={everyDay} today={launcher.today} span={SPAN} height={26} />
				</div>
				<p class="mt-1 text-[0.65rem] text-muted-foreground-dim">{t("played.fortnight")}</p>
			</div>
		</div>
	{/if}

	<!--
		Always drawn, session or not: the settings button lives in here, and a launcher whose
		settings are unreachable until an account resolves has no way to set the account up.
	-->
	<div class="border-t border-border/60 px-2 py-2">
		<div class="flex items-center gap-2.5 px-2 py-1">
			<!-- The face is the way to the skin: it is what changes, so it is what to click. -->
			<button
				type="button"
				onclick={() => (skinOpen = true)}
				aria-label={t("skin.open")}
				title={t("skin.open")}
				class="-mx-1 flex min-w-0 flex-1 items-center gap-2.5 rounded-md px-1 py-0.5 text-left
				       outline-none transition-colors hover:bg-accent/40 focus-visible:ring-3 focus-visible:ring-ring/50"
			>
				<PlayerHead skin={launcher.skinTexture} fallback={launcher.playerHead} size={40} />

				<span class="min-w-0 flex-1">
					<span class="block truncate text-sm">
						{launcher.session?.name ?? t("common.loading")}
					</span>
					{#if launcher.session?.kind === "offline"}
						<span class="block truncate text-xs text-warn" title={t("status.notSignedInHint")}>
							{t("status.notSignedIn")}
						</span>
					{/if}
				</span>
			</button>

			<!-- The gear keeps its size; the button gets one, so the target matches the face beside it. -->
			<button
				type="button"
				onclick={onsettings}
				class="flex size-8 items-center justify-center shrink-0 rounded-md text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
				aria-label={t("nav.settings")}
				title={t("nav.settings")}
			>
				<Settings class="size-4" />
			</button>
		</div>

		<!--
			Under the account rather than beside it: signing in is a sentence about the line
			above it, and a second icon in that row would crowd the settings gear.
		-->
		{#if launcher.session?.kind === "offline"}
			<Button
				variant="secondary"
				size="sm"
				class="mt-1.5 w-full"
				disabled={launcher.signingIn}
				onclick={() => launcher.signIn()}
			>
				{#if launcher.signingIn}
					<Loader2 class="size-4 animate-spin" />
					{t("signIn.waitingShort")}
				{:else}
					<LogIn class="size-4" />
					{t("signIn.action")}
				{/if}
			</Button>
		{/if}
	</div>

	<SkinDialog bind:open={skinOpen} />
	<SignInDialog />
</nav>
