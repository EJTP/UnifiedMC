<script lang="ts">
	import { Loader2, TriangleAlert } from "@lucide/svelte";
	import * as Dialog from "$lib/components/ui/dialog";
	import * as Select from "$lib/components/ui/select";
	import { Button } from "$lib/components/ui/button";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";
	import type { SavedServer } from "$lib/types";

	/** Which server is being set up; null closes the dialog. Open state lives with the caller. */
	let {
		server,
		onclose
	}: {
		server: SavedServer | null;
		onclose: () => void;
	} = $props();

	const open = $derived(server !== null);

	/** Sentinels, never "": bits-ui reads the empty string as "no value" and locks up on it. */
	const AUTO = "auto";
	const NONE = "vanilla";

	/** What the row currently reports - the published manifest, or the player's own choice. */
	const manifest = $derived(server ? (launcher.status[server.id]?.manifest ?? null) : null);

	let minecraft = $state(AUTO);
	let loader = $state(NONE);
	let saving = $state(false);
	let failure = $state<string | null>(null);

	// Every opening starts from what is stored, not from what the last one left behind.
	$effect(() => {
		if (!server) return;
		minecraft = server.minecraft ?? AUTO;
		loader = server.loader ?? NONE;
		failure = null;
		saving = false;
	});

	const loaders = $derived([
		{ id: NONE, label: t("setup.loaderNone") },
		{ id: "fabric", label: "Fabric" },
		{ id: "neoforge", label: "NeoForge" },
		{ id: "forge", label: "Forge" },
		{ id: "quilt", label: "Quilt" }
	]);

	const loaderLabel = $derived(loaders.find((entry) => entry.id === loader)?.label ?? loader);

	const detected = $derived(
		manifest
			? [manifest.minecraft, manifest.loader?.type ?? t("instances.noMods")].join("  ·  ")
			: t("setup.undetected")
	);

	async function save() {
		if (!server || saving) return;
		saving = true;
		failure = null;
		launcher.error = null;
		await launcher.configure(
			server,
			minecraft === AUTO ? null : minecraft,
			loader === NONE ? null : loader
		);
		saving = false;

		// configure() reports by writing launcher.error rather than throwing. A combination that
		// cannot be installed has to stay on screen here instead of closing as if it was taken -
		// and it is taken out of the page-wide banner, which would otherwise say it twice.
		if (launcher.error) {
			failure = launcher.error;
			launcher.error = null;
			return;
		}
		onclose();
	}
</script>

<Dialog.Root {open} onOpenChange={(next) => !next && onclose()}>
	<Dialog.Content class="sm:max-w-[460px]">
		<Dialog.Header>
			<Dialog.Title>{t("setup.title")}</Dialog.Title>
			<Dialog.Description class="truncate">{server?.name ?? ""}</Dialog.Description>
		</Dialog.Header>

		<!--
			A scroll area, not a plain box. The window may be as short as 520px and this dialog
			is taller than that: without it the loader picker and the failure box below it sit
			behind the footer with no way to reach them.
		-->
		<div class="max-h-[58vh] space-y-4 overflow-x-hidden overflow-y-auto px-1 py-2">
			<p class="text-xs leading-relaxed text-muted-foreground">{t("setup.description")}</p>

			<div class="flex items-baseline gap-2 rounded-lg bg-muted/50 px-3 py-2">
				<span class="shrink-0 text-xs text-muted-foreground">{t("setup.detected")}</span>
				<span class="min-w-0 flex-1 truncate text-right font-mono text-xs">{detected}</span>
			</div>

			<div class="space-y-2">
				<span class="block text-sm font-medium">{t("setup.version")}</span>
				<Select.Root type="single" bind:value={minecraft}>
					<Select.Trigger aria-label={t("setup.version")}>
						<span>{minecraft === AUTO ? t("setup.versionAuto") : minecraft}</span>
					</Select.Trigger>
					<Select.Content>
						<Select.Item
							value={AUTO}
							label={t("setup.versionAuto")}
							hint={manifest
								? t("setup.versionDetected", { version: manifest.minecraft })
								: undefined}
						/>
						{#if launcher.versions.length === 0}
							<Select.Item value="__none" label={t("instances.versionsUnavailable")} disabled />
						{/if}
						{#each launcher.versions as option (option)}
							<Select.Item
								value={option}
								label={option}
								mono
								badge={option === manifest?.minecraft ? t("common.detected") : undefined}
							/>
						{/each}
					</Select.Content>
				</Select.Root>
				<p class="text-xs leading-relaxed text-muted-foreground">{t("setup.hint.proxy")}</p>
			</div>

			<div class="space-y-2">
				<span class="block text-sm font-medium">{t("setup.loader")}</span>
				<Select.Root type="single" bind:value={loader}>
					<Select.Trigger aria-label={t("setup.loader")}>
						<span>{loaderLabel}</span>
					</Select.Trigger>
					<Select.Content>
						{#each loaders as option (option.id)}
							<Select.Item value={option.id} label={option.label} />
						{/each}
					</Select.Content>
				</Select.Root>
				<p class="text-xs leading-relaxed text-muted-foreground">{t("setup.hint.vanilla")}</p>
			</div>

			{#if failure}
				<div
					class="flex items-start gap-2.5 rounded-lg border border-destructive/40
					       bg-destructive/10 px-3 py-2.5"
				>
					<TriangleAlert class="mt-0.5 size-4 shrink-0 text-destructive" />
					<p class="min-w-0 text-xs leading-relaxed break-words text-destructive-foreground/90">
						{failure}
					</p>
				</div>
			{/if}
		</div>

		<Dialog.Footer>
			<Button variant="ghost" onclick={onclose}>{t("common.cancel")}</Button>
			<Button onclick={save} disabled={saving}>
				{#if saving}
					<Loader2 class="size-4 animate-spin" />
					{t("setup.checking")}
				{:else}
					{t("setup.save")}
				{/if}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
