<script lang="ts">
	import { Boxes, Server, Settings } from "@lucide/svelte";
	import { t } from "$lib/i18n.svelte";
	import { launcher, type View } from "$lib/state.svelte";
	import { cn } from "$lib/utils";

	let { onsettings }: { onsettings: () => void } = $props();

	const items: { id: View; key: string; icon: typeof Server; count: () => number }[] = [
		{ id: "servers", key: "nav.servers", icon: Server, count: () => launcher.servers.length },
		{ id: "instances", key: "nav.instances", icon: Boxes, count: () => launcher.instances.length }
	];
</script>

<!--
	data-tauri-drag-region on the top strip only: the whole sidebar as a window handle would
	swallow the clicks on everything in it.

	One inset all the way down - px-2 outside, px-2 inside every row - so the logo, the nav
	icons, the counts and the account line up on the same two edges instead of three rhythms.
-->
<nav class="flex w-52 shrink-0 flex-col border-r border-border/60 bg-card/40">
	<div data-tauri-drag-region class="flex h-11 shrink-0 items-center gap-2.5 px-4">
		<img src="/mark.png" alt="" class="pointer-events-none size-5" />
		<span class="pointer-events-none truncate text-base font-semibold">UnifiedMC</span>
	</div>

	<div class="flex flex-col gap-0.5 px-2 pt-3">
		{#each items as item (item.id)}
			{@const active = launcher.view === item.id}
			<button
				type="button"
				aria-current={active ? "page" : undefined}
				onclick={() => (launcher.view = item.id)}
				class={cn(
					"flex items-center gap-2.5 rounded-md px-2 py-2 text-sm outline-none transition-colors duration-150",
					"focus-visible:ring-3 focus-visible:ring-ring/50",
					active
						? "bg-secondary text-foreground"
						: "text-muted-foreground hover:bg-accent/50 hover:text-foreground"
				)}
			>
				<item.icon class="size-4 shrink-0" />
				<span class="flex-1 truncate text-left">{t(item.key)}</span>
				{#if item.count() > 0}
					<span class="shrink-0 font-mono text-xs text-muted-foreground/70">{item.count()}</span>
				{/if}
			</button>
		{/each}
	</div>

	<div class="flex-1"></div>

	<!--
		Always drawn, session or not: the settings button lives in here, and a launcher whose
		settings are unreachable until an account resolves has no way to set the account up.
	-->
	<div class="border-t border-border/60 px-2 py-2">
		<div class="flex items-center gap-2.5 px-2 py-1">
			{#if launcher.playerHead}
				<!-- pixelated: a face is eight pixels wide, and smoothing it is mush -->
				<img
					src={launcher.playerHead}
					alt=""
					class="size-7 shrink-0 rounded-sm [image-rendering:pixelated]"
				/>
			{:else}
				<div class="size-7 shrink-0 rounded-sm bg-muted"></div>
			{/if}

			<div class="min-w-0 flex-1">
				<p class="truncate text-sm">{launcher.session?.name ?? t("common.loading")}</p>
				{#if launcher.session?.kind === "offline"}
					<p class="truncate text-xs text-warn" title={t("status.notSignedInHint")}>
						{t("status.notSignedIn")}
					</p>
				{/if}
			</div>

			<button
				type="button"
				onclick={onsettings}
				class="shrink-0 rounded-md text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
				aria-label={t("nav.settings")}
				title={t("nav.settings")}
			>
				<Settings class="size-4" />
			</button>
		</div>
	</div>
</nav>
