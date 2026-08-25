<script lang="ts">
	import { Boxes, Gamepad2, Loader2, Package, Play, Trash2 } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";
	import type { Instance } from "$lib/types";

	let {
		instance,
		busy = false,
		onplay,
		onmods,
		onremove
	}: {
		instance: Instance;
		busy?: boolean;
		onplay: () => void;
		onmods: () => void;
		onremove: () => void;
	} = $props();

	const facts = $derived(
		[instance.minecraft, instance.loader ?? t("instances.noMods"), instance.source]
			.filter(Boolean)
			.join("  ·  ")
	);

	/** No other row may start while one is starting; the overlay covers the list either way. */
	const blocked = $derived(Boolean(launcher.playing) && !busy);

	/** Already running: the row says so rather than offering a start that is refused. */
	const live = $derived(Boolean(launcher.running[instance.id]));
</script>

<!-- Same box, icon and button geometry as ServerCard: the two lists read as one system. -->
<article
	class="group flex items-center gap-3.5 rounded-lg border border-border/70 bg-card px-4 py-3
	       transition-colors duration-200 hover:border-border hover:bg-accent/40"
>
	<!-- The width a server row spends on its state dot, so both lists start their icons at one x. -->
	<span class="size-3 shrink-0" aria-hidden="true"></span>

	<div class="flex size-11 shrink-0 items-center justify-center overflow-hidden bg-muted">
		<Boxes class="size-5 text-muted-foreground/50" />
	</div>

	<div class="min-w-0 flex-1">
		<h3 class="truncate text-base font-medium" title={instance.name}>{instance.name}</h3>
		<p class="mt-0.5 truncate text-xs text-muted-foreground" title={facts}>{facts}</p>
	</div>

	<!--
		One cluster with the same gaps as a server row, every slot always occupied: an instance
		without a loader has no mods button, and its play button must not slide right because
		of it.
	-->
	<div class="flex shrink-0 items-center gap-1">
		<Button
			variant="ghost"
			size="icon"
			class="text-muted-foreground opacity-0 transition-opacity
			       group-hover:opacity-100 hover:text-destructive focus-visible:opacity-100"
			onclick={onremove}
			aria-label={t("instances.action.remove", { name: instance.name })}
		>
			<Trash2 class="size-4" />
		</Button>

		{#if instance.loader}
			<Button
				variant="ghost"
				size="icon"
				class="text-muted-foreground hover:text-foreground"
				onclick={onmods}
				title={t("mods.title")}
				aria-label={t("instances.action.mods", { name: instance.name })}
			>
				<Package class="size-4" />
			</Button>
		{:else}
			<div class="size-8" aria-hidden="true"></div>
		{/if}

		<Button
			size="lg"
			class="min-w-28 bg-cta text-cta-foreground hover:bg-cta/90"
			onclick={onplay}
			disabled={busy || blocked || live}
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
