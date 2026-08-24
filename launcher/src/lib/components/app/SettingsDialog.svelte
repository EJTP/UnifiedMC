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
	let port = $state("");
	/** The size the slider stands on, kept while Auto is on so switching back returns to it. */
	let heap = $state(4096);
	let tab = $state("general");

	$effect(() => {
		if (!open) return;
		draft = { ...launcher.settings };
		tab = "general";
		port = String(launcher.settings.manifest_port);
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
	 * Only sizes this machine can actually back.
	 *
	 * Past physical memory the stalling moves from the garbage collector into the swap file,
	 * so offering 16 GB on an 8 GB machine offers a worse experience, not a better one. The
	 * floor keeps one step of travel on a small machine; Rust clamps the value again anyway.
	 */
	function ceiling(mb: number) {
		return Math.max(MIN_MB + STEP_MB, Math.floor((mb - 2048) / STEP_MB) * STEP_MB);
	}

	/**
	 * Never below what is already configured. Until machine_memory answers, this machine looks
	 * like the smallest one there is - and a slider whose maximum is under its value drags the
	 * value down to it, quietly turning a stored 8 GB into 2.5 GB.
	 */
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

	/**
	 * The flags the JVM will really get, from the same Rust code the launch path uses, so a
	 * profile can never promise something play.rs does not pass. Debounced: dragging the
	 * slider would otherwise fire one call per step.
	 */
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

	/**
	 * What Auto settled on, read back off the preview rather than guessed here: `heap` is only
	 * the position the slider is parked at, and printing that under Auto would put a number on
	 * screen that the -Xmx three lines below it contradicts.
	 */
	const autoMb = $derived(Number(flags.find((f) => f.startsWith("-Xmx"))?.match(/\d+/)?.[0] ?? 0));

	const portValue = $derived(Number(port));
	const portValid = $derived(
		port.trim() !== "" && Number.isInteger(portValue) && portValue >= 1 && portValue <= 65535
	);

	async function save() {
		if (!portValid) {
			tab = "advanced";
			return;
		}
		await launcher.saveSettings({ ...$state.snapshot(draft), manifest_port: portValue });
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
			-mr-[10px] with a stable gutter on every tab body: the scrollbar is 10px wide, and
			letting it eat into the fields instead of into the padding is what put the tab strip
			and the fields under it on two different right edges - and moved them again whenever
			a tab happened to be long enough to scroll.
		-->
		<Tabs.Root bind:value={tab} class="flex min-h-0 flex-col">
			<Tabs.List class="w-full">
				<Tabs.Trigger value="general">{t("settings.tab.general")}</Tabs.Trigger>
				<Tabs.Trigger value="java">{t("settings.tab.java")}</Tabs.Trigger>
				<Tabs.Trigger value="advanced">{t("settings.tab.advanced")}</Tabs.Trigger>
			</Tabs.List>

			<Tabs.Content value="general" class="min-h-0 space-y-4 overflow-y-auto py-2 -mr-[10px] [scrollbar-gutter:stable]">
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
			</Tabs.Content>

			<Tabs.Content value="java" class="min-h-0 space-y-4 overflow-y-auto py-2 -mr-[10px] [scrollbar-gutter:stable]">
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

			<Tabs.Content value="advanced" class="min-h-0 space-y-4 overflow-y-auto py-2 -mr-[10px] [scrollbar-gutter:stable]">
				<div class="space-y-2">
					<label for="settings-manifest-port" class="block text-sm font-medium">
						{t("settings.manifestPort")}
					</label>
					<Input
						id="settings-manifest-port"
						type="number"
						min={1}
						max={65535}
						bind:value={port}
						aria-invalid={!portValid}
						aria-describedby="settings-manifest-port-hint"
						class="font-mono"
					/>
					{#if !portValid}
						<p class="text-xs break-words text-destructive">{t("settings.manifestPortInvalid")}</p>
					{/if}
					<p id="settings-manifest-port-hint" class="text-xs break-words text-muted-foreground">
						{t("settings.manifestPortHint")}
					</p>
				</div>

				<div class="space-y-2">
					<label for="settings-curseforge-key" class="block text-sm font-medium">
						{t("settings.curseforgeKey")}
					</label>
					<Input
						id="settings-curseforge-key"
						type="password"
						autocomplete="off"
						bind:value={draft.curseforge_key}
						class="font-mono"
					/>
					<p class="text-xs text-muted-foreground">{t("settings.curseforgeKeyHint")}</p>
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
		</Tabs.Root>

		<Dialog.Footer>
			<Button variant="ghost" onclick={() => (open = false)}>{t("common.cancel")}</Button>
			<Button onclick={save}>{t("common.save")}</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
