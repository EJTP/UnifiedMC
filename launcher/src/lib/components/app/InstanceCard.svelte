<script lang="ts">
	import { Boxes, Clock, Gamepad2, Loader2, Package, Play, Trash2 } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";
	import { ago, spell } from "$lib/played";
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

	const played = $derived(launcher.played(`instance:${instance.id}`));

	/** Pills, matching a server row: the two lists are the same object at different distances. */
	const chips = $derived(
		[
			{ text: instance.minecraft, strong: true },
			{ text: instance.loader ?? t("instances.noMods"), strong: false },
			...(instance.source ? [{ text: instance.source, strong: false }] : [])
		]
	);

	/** No other row may start while one is starting; the overlay covers the list either way. */
	const blocked = $derived(Boolean(launcher.playing) && !busy);

	/** Already running: the row says so rather than offering a start that is refused. */
	const live = $derived(Boolean(launcher.running[instance.id]));
</script>

<!-- Same box, icon and button geometry as ServerCard: the two lists read as one system. -->
<article class="surface surface-hover group flex items-center gap-3.5 py-3 pr-4 pl-5">
	<!-- The strip a server row carries its state on, kept neutral: an instance has no state. -->
	<span aria-hidden="true" class="absolute inset-y-0 left-0 w-[3px] bg-border/60"></span>

	<div class="flex size-11 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-muted/70">
		<Boxes class="size-5 text-muted-foreground-dim" />
	</div>

	<div class="min-w-0 flex-1">
		<h3 class="truncate text-base font-semibold" title={instance.name}>{instance.name}</h3>
		<div class="mt-1 flex flex-wrap items-center gap-1">
			{#each chips as chip, i (i)}
				<span class="chip {chip.strong ? 'chip-strong' : ''}">{chip.text}</span>
			{/each}

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
			class="cta-glow bg-cta text-cta-foreground hover:bg-cta/90 min-w-28"
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
