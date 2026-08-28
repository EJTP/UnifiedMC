<script lang="ts">
	import * as Dialog from "$lib/components/ui/dialog";
	import * as Tabs from "$lib/components/ui/tabs";
	import * as Select from "$lib/components/ui/select";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import { Slider } from "$lib/components/ui/slider";
	import { Switch } from "$lib/components/ui/switch";
	import { call } from "$lib/bridge";
	import { launcher } from "$lib/state.svelte";
	import { t, LOCALES } from "$lib/i18n.svelte";
	import {
		ACCENTS,
		BACKDROPS,
		CUSTOM,
		READABLE,
		applyTheme,
		pairOf,
		whiteContrast
	} from "$lib/theme";
	import type { Settings } from "$lib/types";

	let { open = $bindable(false) }: { open: boolean } = $props();

	/** Edited locally, written on save - so cancelling really cancels. */
	let draft = $state<Settings>({ ...launcher.settings });
	let machineMb = $state(0);
	let dataDir = $state("");
	let flags = $state<string[]>([]);
	/** The preview is a convenience; when the command refuses, the box has to say so. */
	let flagsFailed = $state(false);
	/** Held as text: a number input reports NaN while it is empty, and NaN in the field reads as broken. */
	/** The size the slider stands on, kept while Auto is on so switching back returns to it. */
	let heap = $state(4096);
	let tab = $state("general");

	/**
	 * The accent follows the swatch straight away rather than waiting for save: it repaints
	 * the whole window, and picking a colour you cannot see until you commit to it is not
	 * picking a colour. Cancelling puts the saved one back.
	 */
	$effect(() => {
		applyTheme(open ? draft : launcher.settings);
	});

	/** What the two swatches are showing, preset or custom. */
	const pair = $derived(pairOf(draft));

	/**
	 * White sits on both accents, so a colour picked too light makes its own label disappear.
	 * Said rather than refused - it is the player's window - but said plainly.
	 */
	const faint = $derived(
		[
			{ what: t("accent.custom.primary"), ratio: whiteContrast(pair.primary) },
			{ what: t("accent.custom.action"), ratio: whiteContrast(pair.cta) }
		].filter((c) => c.ratio < READABLE)
	);

	/** Picking a colour switches to custom, keeping whatever was already there. */
	function custom(which: "primary" | "cta", value: string) {
		if (which === "primary") draft.accent_primary = value;
		else draft.accent_cta = value;
		draft.accent = CUSTOM;
	}

	$effect(() => {
		if (!open) return;
		draft = { ...launcher.settings };
		tab = "general";
		heap = launcher.settings.memory || 4096;
		void call<number>("machine_memory")
			.then((mb) => {
				machineMb = mb;
				heap = Math.min(Math.max(heap, MIN_MB), ceiling(mb));
			})
			// unknown memory is not a reason to fail; the ceiling below falls back to what is set
			.catch(() => (machineMb = 0));
		// Only if the backend offers it - the path is a nicety, not something to fail over.
		void call<string>("data_dir")
			.then((dir) => (dataDir = dir ?? ""))
			.catch(() => (dataDir = ""));
	});

	const MIN_MB = 2048;
	const STEP_MB = 512;

	/**
	 * Only sizes this machine can back. Past physical memory the stalling moves from the garbage
	 * collector into the swap file.
	 */
	function ceiling(mb: number) {
		return Math.max(MIN_MB + STEP_MB, Math.floor((mb - 2048) / STEP_MB) * STEP_MB);
	}

	/** Never below what is already configured, or the first paint would clamp a real setting. */
	const maxMb = $derived(Math.max(ceiling(machineMb), heap, draft.memory));
	const auto = $derived(draft.memory === 0);

	const profiles = [
		{ id: "balanced", label: "settings.gc.balanced", hint: "settings.gc.balancedHint" },
		{ id: "throughput", label: "settings.gc.throughput", hint: "settings.gc.throughputHint" },
		{ id: "default", label: "settings.gc.default", hint: "settings.gc.defaultHint" },
		{ id: "custom", label: "settings.gc.custom", hint: "settings.gc.customHint" }
	];
	const profile = $derived(profiles.find((p) => p.id === draft.jvm_profile) ?? profiles[0]);

	const languages = $derived([
		{ id: "system", label: t("settings.languageSystem") },
		...LOCALES.map((entry) => ({ id: entry.id, label: entry.label }))
	]);
	const language = $derived(
		languages.find((entry) => entry.id === draft.language) ?? languages[0]
	);

	function gb(mb: number) {
		return mb % 1024 === 0 ? `${mb / 1024}` : (mb / 1024).toFixed(1);
	}

	/** The flags the JVM really gets, from the same Rust the launch path uses. */
	$effect(() => {
		if (!open) return;
		const settings = $state.snapshot(draft);
		const id = setTimeout(() => {
			void call<string[]>("jvm_preview", { settings, mods: 0 })
				.then((list) => {
					flags = list;
					flagsFailed = false;
				})
				.catch(() => {
					flags = [];
					flagsFailed = true;
				});
		}, 200);
		return () => clearTimeout(id);
	});

	/** What Auto settled on, read off the preview: `heap` is only where the slider sits. */
	const autoMb = $derived(Number(flags.find((f) => f.startsWith("-Xmx"))?.match(/\d+/)?.[0] ?? 0));


	async function save() {
		await launcher.saveSettings($state.snapshot(draft));
		open = false;
	}
</script>

<Dialog.Root bind:open>
	<!-- Fixed box, one scrolling body: no tab may push the dialog past the window. -->
	<Dialog.Content
		class="sm:max-w-[520px] grid h-[34rem] max-h-[calc(100vh-3rem)] grid-rows-[auto_minmax(0,1fr)_auto]"
	>
		<Dialog.Header>
			<Dialog.Title>{t("settings.title")}</Dialog.Title>
		</Dialog.Header>

		<!--
			A stable gutter on every tab body, so the fields do not shift sideways the moment a
			tab is long enough to scroll.

			The gutter used to be bought with -mr-[10px], which pulled the scrollbar out to the
			dialog's edge - and made the body ten pixels wider than the box holding it, which
			is a horizontal scrollbar on the dialog itself. The scrollbar sits inside now.
		-->
		<Tabs.Root bind:value={tab} class="flex min-h-0 flex-col">
			<Tabs.List class="w-full">
				<Tabs.Trigger value="general">{t("settings.tab.general")}</Tabs.Trigger>
				<Tabs.Trigger value="java">{t("settings.tab.java")}</Tabs.Trigger>
			</Tabs.List>

			<Tabs.Content value="general" class="min-h-0 space-y-4 overflow-y-auto py-2 pr-1 [scrollbar-gutter:stable]">
				<div class="space-y-2">
					<span id="settings-language" class="block text-sm font-medium">
						{t("settings.language")}
					</span>
					<Select.Root type="single" bind:value={draft.language}>
						<Select.Trigger aria-labelledby="settings-language">
							<span>{language.label}</span>
						</Select.Trigger>
						<Select.Content>
							{#each languages as entry (entry.id)}
								<Select.Item value={entry.id} label={entry.label} />
							{/each}
						</Select.Content>
					</Select.Root>
				</div>

				<div class="space-y-2">
					<label for="settings-offline-name" class="block text-sm font-medium">
						{t("settings.offlineName")}
					</label>
					<Input id="settings-offline-name" bind:value={draft.offline_name} />
					<p class="text-xs text-muted-foreground">{t("settings.offlineNameHint")}</p>
				</div>

				<div class="space-y-2">
					<span class="block text-sm font-medium">{t("accent.title")}</span>
					<!--
						Swatches, not a dropdown: this is the one setting whose whole point is what
						it looks like, and a list of colour names would be a worse way to say it.
					-->
					<div class="flex flex-wrap gap-2" role="radiogroup" aria-label={t("accent.title")}>
						{#each ACCENTS as accent (accent.id)}
							{@const picked = draft.accent === accent.id}
							<button
								type="button"
								role="radio"
								aria-checked={picked}
								aria-label={t(accent.key)}
								title={t(accent.key)}
								onclick={() => (draft.accent = accent.id)}
								class="relative size-8 rounded-full outline-none transition-transform
								       hover:scale-110 focus-visible:ring-3 focus-visible:ring-ring/50
								       {picked ? 'ring-2 ring-foreground/80 ring-offset-2 ring-offset-popover' : ''}"
								style="background: linear-gradient(135deg, {accent.primary} 0 50%, {accent.cta} 50% 100%)"
							></button>
						{/each}

						<!--
							The two native pickers are the custom option: choosing a colour in one is
							what selects it, so there is no separate "custom" button to press first.
						-->
						<label
							class="flex items-center gap-1 rounded-full px-1.5 outline-none
							       {draft.accent === CUSTOM
								? 'ring-2 ring-foreground/80 ring-offset-2 ring-offset-popover'
								: ''}"
							title={t("accent.custom.title")}
						>
							<input
								type="color"
								value={pair.primary}
								oninput={(e) => custom("primary", e.currentTarget.value)}
								aria-label={t("accent.custom.primary")}
								class="size-7 cursor-pointer rounded-full border-0 bg-transparent p-0
								       [&::-webkit-color-swatch-wrapper]:p-0
								       [&::-webkit-color-swatch]:rounded-full [&::-webkit-color-swatch]:border-0"
							/>
							<input
								type="color"
								value={pair.cta}
								oninput={(e) => custom("cta", e.currentTarget.value)}
								aria-label={t("accent.custom.action")}
								class="size-7 cursor-pointer rounded-full border-0 bg-transparent p-0
								       [&::-webkit-color-swatch-wrapper]:p-0
								       [&::-webkit-color-swatch]:rounded-full [&::-webkit-color-swatch]:border-0"
							/>
						</label>
					</div>
					<p class="text-xs text-muted-foreground">{t("accent.hint")}</p>

					{#if faint.length > 0}
						<p class="text-xs text-warn">
							{t("accent.faint", { what: faint.map((c) => c.what).join(", ") })}
						</p>
					{/if}
				</div>

				<div class="space-y-2">
					<span class="block text-sm font-medium">{t("backdrop.title")}</span>
					<div class="flex flex-wrap gap-2" role="radiogroup" aria-label={t("backdrop.title")}>
						{#each BACKDROPS as surface (surface.id)}
							{@const picked = draft.backdrop === surface.id}
							<button
								type="button"
								role="radio"
								aria-checked={picked}
								aria-label={t(surface.key)}
								title={t(surface.key)}
								onclick={() => (draft.backdrop = surface.id)}
								class="size-8 rounded-lg border border-border/70 outline-none transition-transform
								       hover:scale-110 focus-visible:ring-3 focus-visible:ring-ring/50
								       {picked ? 'ring-2 ring-foreground/80 ring-offset-2 ring-offset-popover' : ''}"
								style="background: linear-gradient(160deg, {surface.card} 0 55%, {surface.background} 55% 100%)"
							></button>
						{/each}
					</div>
					<p class="text-xs text-muted-foreground">{t("backdrop.hint")}</p>
				</div>

				<div class="flex items-start justify-between gap-4">
					<div class="min-w-0 space-y-1">
						<span id="settings-keep-open" class="block text-sm font-medium">
							{t("settings.keepOpen")}
						</span>
						<p class="text-xs text-muted-foreground">{t("settings.keepOpenHint")}</p>
					</div>
					<Switch
						aria-labelledby="settings-keep-open"
						checked={draft.keep_open}
						onCheckedChange={(next) => (draft.keep_open = next)}
					/>
				</div>
							{#if dataDir}
					<div class="space-y-2">
						<label for="settings-data-dir" class="block text-sm font-medium">
							{t("settings.dataDir")}
						</label>
						<Input
							id="settings-data-dir"
							value={dataDir}
							readonly
							class="font-mono text-xs text-muted-foreground"
						/>
					</div>
				{/if}
			</Tabs.Content>

			<Tabs.Content value="java" class="min-h-0 space-y-4 overflow-y-auto py-2 pr-1 [scrollbar-gutter:stable]">
				<div class="space-y-2.5">
					<div class="flex items-baseline justify-between gap-3">
						<span id="settings-memory" class="text-sm font-medium">
							{t("settings.memory")}
						</span>
						<span class="truncate text-xs text-muted-foreground">
							{machineMb ? t("settings.machineMemory", { gb: Math.round(machineMb / 1024) }) : ""}
						</span>
					</div>

					<div class="flex items-center justify-between gap-4">
						<!--
							The number even under Auto: the switch beside it already says "Auto", and
							reading that word twice on one line told the player nothing - what they
							want to know there is how much Auto actually decided on.
						-->
						<span class="font-mono text-sm">
							{#if auto}
								{autoMb
									? t("settings.heapValue", { mb: autoMb, gb: gb(autoMb) })
									: t("settings.memoryAuto")}
							{:else}
								{t("settings.heapValue", { mb: heap, gb: gb(heap) })}
							{/if}
						</span>
						<div class="flex items-center gap-2">
							<span id="settings-memory-auto" class="text-xs text-muted-foreground">
								{t("settings.memoryAuto")}
							</span>
							<Switch
								aria-labelledby="settings-memory-auto"
								checked={auto}
								onCheckedChange={(next) => (draft.memory = next ? 0 : heap)}
							/>
						</div>
					</div>

					<!-- The thumb carries no name of its own, so the group gives it one. -->
					<div role="group" aria-labelledby="settings-memory">
						<Slider
							type="single"
							min={MIN_MB}
							max={maxMb}
							step={STEP_MB}
							disabled={auto}
							value={auto ? heap : draft.memory}
							onValueChange={(next: number) => {
								heap = next;
								if (!auto) draft.memory = next;
							}}
						/>
					</div>
					<p class="text-xs text-muted-foreground">{t("settings.memoryHint")}</p>
				</div>

				<div class="space-y-2">
					<span id="settings-gc" class="block text-sm font-medium">{t("settings.gc")}</span>
					<Select.Root type="single" bind:value={draft.jvm_profile}>
						<Select.Trigger aria-labelledby="settings-gc">
							<span>{t(profile.label)}</span>
						</Select.Trigger>
						<Select.Content>
							{#each profiles as entry (entry.id)}
								<Select.Item value={entry.id} label={t(entry.label)} hint={t(entry.hint)} />
							{/each}
						</Select.Content>
					</Select.Root>
					<p class="text-xs text-muted-foreground">{t(profile.hint)}</p>
				</div>

				{#if draft.jvm_profile === "custom"}
					<div class="space-y-2">
						<label for="settings-jvm-args" class="block text-sm font-medium">
							{t("settings.customArgs")}
						</label>
						<Input
							id="settings-jvm-args"
							bind:value={draft.jvm_args}
							placeholder="-XX:+UseG1GC -Xss2M"
							class="font-mono text-xs"
						/>
					</div>
				{/if}

				<div class="space-y-2">
					<span class="block text-sm font-medium">{t("settings.flags")}</span>
					<pre
						class="max-h-40 overflow-y-auto rounded-lg border border-border bg-muted/40 p-2.5 font-mono text-[0.7rem] leading-5 break-all whitespace-pre-wrap text-muted-foreground">{flagsFailed
							? t("settings.flagsUnavailable")
							: flags.length
								? flags.join("\n")
								: t("common.loading")}</pre>
					<p class="text-xs text-muted-foreground">
						{t("settings.flagsHint")}
						{#if auto}{" "}{t("settings.flagsAutoHint")}{/if}
					</p>
				</div>
			</Tabs.Content>
		</Tabs.Root>

		<Dialog.Footer>
			<Button variant="ghost" onclick={() => (open = false)}>{t("common.cancel")}</Button>
			<Button onclick={save}>{t("common.save")}</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
