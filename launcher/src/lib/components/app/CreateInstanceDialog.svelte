<script lang="ts">
	import { TriangleAlert } from "@lucide/svelte";
	import * as Dialog from "$lib/components/ui/dialog";
	import * as Select from "$lib/components/ui/select";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import { call } from "$lib/bridge";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";

	let {
		open = $bindable(false),
		minecraft: prefillMinecraft = null,
		loader: prefillLoader = null,
		oncreated
	}: {
		open: boolean;
		/** What the server runs, when the dialog is opened to build a profile for one. */
		minecraft?: string | null;
		loader?: string | null;
		oncreated?: (id: string) => void;
	} = $props();

	/** "vanilla" rather than "" throughout: an empty value reads as "nothing picked" to Select. */
	const NONE = "vanilla";
	/** Same reasoning for the loader build: "latest" is a choice, "" would look like none. */
	const LATEST = "latest";

	let name = $state("");
	let minecraft = $state("");
	let loader = $state(NONE);
	let loaderVersion = $state(LATEST);
	let loaderVersions = $state<string[]>([]);
	let loading = $state(false);

	// derived, not a plain array: the labels have to follow a language change
	const loaders = $derived([
		{ id: NONE, label: t("setup.loaderNone") },
		{ id: "fabric", label: "Fabric" },
		{ id: "neoforge", label: "NeoForge" },
		{ id: "forge", label: "Forge" },
		{ id: "quilt", label: "Quilt" }
	]);

	$effect(() => {
		if (!open) return;
		name = "";
		minecraft = prefillMinecraft ?? "";
		loader = prefillLoader ?? NONE;
		loaderVersion = LATEST;
		loaderVersions = [];
		launcher.error = null;
	});

	/** Only the newest lookup may write the list; a slow earlier one must not overwrite it. */
	let request = 0;

	$effect(() => {
		const [forLoader, forMinecraft] = [loader, minecraft];
		const mine = ++request;
		loaderVersion = LATEST;
		if (forLoader === NONE || !forMinecraft) {
			loaderVersions = [];
			loading = false;
			return;
		}
		loading = true;
		void call<string[] | null>("loader_versions", {
			loader: forLoader,
			minecraft: forMinecraft
		})
			.then((list) => {
				if (mine !== request) return;
				loaderVersions = list ?? [];
			})
			.catch(() => {
				if (mine !== request) return;
				loaderVersions = [];
			})
			.finally(() => {
				if (mine === request) loading = false;
			});
	});

	async function create(event: SubmitEvent) {
		event.preventDefault();
		if (!minecraft) return;
		const id = await launcher.addInstance(
			name,
			minecraft,
			loader === NONE ? null : loader,
			loaderVersion === LATEST ? null : loaderVersion
		);
		// Staying open with the reason on screen; closing would throw the whole form away.
		if (!id) return;
		oncreated?.(id);
		open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-[460px]">
		<Dialog.Header>
			<Dialog.Title>{t("instances.create.title")}</Dialog.Title>
		</Dialog.Header>

		<form onsubmit={create}>
			<div class="space-y-4 py-2">
				<div class="space-y-2">
					<label for="create-instance-name" class="block text-sm font-medium">
						{t("instances.name")}
					</label>
					<Input id="create-instance-name" bind:value={name} placeholder={t("common.optional")} />
				</div>

				<div class="space-y-2">
					<span class="block text-sm font-medium">{t("instances.version")}</span>
					<Select.Root type="single" bind:value={minecraft}>
						<Select.Trigger aria-label={t("instances.version")}>
							<span class={minecraft ? "" : "text-muted-foreground"}>
								{minecraft || t("instances.versionPick")}
							</span>
						</Select.Trigger>
						<Select.Content>
							<!-- Mojang's list is a network away; without it there is nothing to pick from. -->
							{#if launcher.versions.length === 0}
								<Select.Item value="__none" label={t("instances.versionsUnavailable")} disabled />
							{/if}
							{#each launcher.versions as option (option)}
								<Select.Item
									value={option}
									label={option}
									hint={option === prefillMinecraft ? t("common.detected") : undefined}
								/>
							{/each}
						</Select.Content>
					</Select.Root>
				</div>

				<div class="space-y-2">
					<span class="block text-sm font-medium">{t("instances.loader")}</span>
					<Select.Root type="single" bind:value={loader}>
						<Select.Trigger aria-label={t("instances.loader")}>
							<span>{loaders.find((o) => o.id === loader)?.label ?? loader}</span>
						</Select.Trigger>
						<Select.Content>
							{#each loaders as option (option.id)}
								<Select.Item value={option.id} label={option.label} />
							{/each}
						</Select.Content>
					</Select.Root>
				</div>

				{#if loader !== NONE}
					<div class="space-y-2">
						<span class="block text-sm font-medium">{t("instances.loaderVersion")}</span>
						<Select.Root type="single" bind:value={loaderVersion} disabled={loading}>
							<Select.Trigger aria-label={t("instances.loaderVersion")}>
								<span>
									{loading
										? t("common.loading")
										: loaderVersion === LATEST
											? t("instances.newest")
											: loaderVersion}
								</span>
							</Select.Trigger>
							<Select.Content>
								<Select.Item value={LATEST} label={t("instances.newest")} />
								{#each loaderVersions as option (option)}
									<Select.Item value={option} label={option} />
								{/each}
							</Select.Content>
						</Select.Root>
					</div>
				{/if}

				{#if launcher.error}
					<div
						class="flex items-start gap-2.5 rounded-lg border border-destructive/40
						       bg-destructive/10 px-3.5 py-2.5"
					>
						<TriangleAlert class="mt-0.5 size-4 shrink-0 text-destructive" />
						<p class="min-w-0 text-xs leading-relaxed break-words text-destructive-foreground/90">
							{launcher.error}
						</p>
					</div>
				{/if}
			</div>

			<Dialog.Footer>
				<Button type="button" variant="ghost" onclick={() => (open = false)}>
					{t("common.cancel")}
				</Button>
				<Button type="submit" disabled={!minecraft}>{t("common.create")}</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
