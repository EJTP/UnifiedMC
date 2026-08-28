<script lang="ts">
	import { FileArchive, Sparkles, TriangleAlert, X, Zap } from "@lucide/svelte";
	import * as Dialog from "$lib/components/ui/dialog";
	import * as Select from "$lib/components/ui/select";
	import { Button } from "$lib/components/ui/button";
	import { Input, NumberField } from "$lib/components/ui/input";
	import { Switch } from "$lib/components/ui/switch";
	import { call } from "$lib/bridge";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";

	let { open = $bindable(false) }: { open: boolean } = $props();

	/** "vanilla" rather than "": an empty value reads as "nothing picked" to Select. */
	const NONE = "vanilla";
	const LATEST = "latest";

	let name = $state("");
	let minecraft = $state("");
	let loader = $state(NONE);
	let loaderVersion = $state(LATEST);
	let loaderVersions = $state<string[]>([]);
	let loadingVersions = $state(false);
	let port = $state(25565);
	let memory = $state(4096);
	let eula = $state(false);
	let publish = $state(true);
	/** A modpack to build from, once one has been picked. Empty is "a bare server". */
	let pack = $state("");

	const loaders = $derived([
		{ id: NONE, label: t("host.loaderVanilla") },
		{ id: "fabric", label: "Fabric" },
		{ id: "neoforge", label: "NeoForge" },
		{ id: "forge", label: "Forge" },
		{ id: "quilt", label: "Quilt" }
	]);

	/** A pack states its own version and loader; asking again would let the two disagree. */
	const fromPack = $derived(pack !== "");

	/** Only these load the publisher mod, so only these can hand a pack to a player. */
	const canPublish = $derived(fromPack || loader !== NONE);

	const packName = $derived(pack.split(/[/\\]/).pop() ?? "");

	/**
	 * What the switch shows. Derived rather than synced: a switch that cannot apply must not
	 * sit there looking as though it will, and an effect writing `publish` would be racing the
	 * one that resets the form - whichever ran last would decide, which is not a rule.
	 */
	const publishing = $derived(publish && canPublish);

	/** Half the machine at most, and never below what a server needs to start at all. */
	const maxMemory = $derived(Math.max(2048, Math.floor((launcher.machineMemory || 8192) / 2)));

	$effect(() => {
		if (!open) return;
		name = "";
		minecraft = "";
		loader = NONE;
		loaderVersion = LATEST;
		loaderVersions = [];
		port = nextFreePort();
		memory = 4096;
		eula = false;
		publish = true;
		pack = "";
		launcher.error = null;
	});

	/** Two servers on one port is a second one that dies on start; step past what is taken. */
	function nextFreePort(): number {
		const taken = new Set(launcher.hosts.map((server) => server.port));
		let candidate = 25565;
		while (taken.has(candidate)) candidate += 10;
		return candidate;
	}

	/** Only the newest lookup may write the list; a slow earlier one must not overwrite it. */
	let request = 0;

	$effect(() => {
		const [forLoader, forMinecraft] = [loader, minecraft];
		const mine = ++request;
		loaderVersion = LATEST;
		if (fromPack || forLoader === NONE || !forMinecraft) {
			loaderVersions = [];
			loadingVersions = false;
			return;
		}
		loadingVersions = true;
		void call<string[] | null>("loader_versions", { loader: forLoader, minecraft: forMinecraft })
			.then((list) => {
				if (mine === request) loaderVersions = list ?? [];
			})
			.catch(() => {
				if (mine === request) loaderVersions = [];
			})
			.finally(() => {
				if (mine === request) loadingVersions = false;
			});
	});

	async function choosePack() {
		const picked = await launcher.pickPack();
		if (!picked) return;
		pack = picked;
		// The pack's own name is the obvious one, and it is still editable.
		if (!name) name = (packName.split(".").slice(0, -1).join(".") || packName).replace(/[-_]/g, " ");
	}

	const ready = $derived(eula && (fromPack || minecraft !== "") && port >= 1024 && port <= 65535);

	async function create(event: SubmitEvent) {
		event.preventDefault();
		if (!ready) return;
		const made = await launcher.createHost({
			name,
			minecraft: fromPack ? "" : minecraft,
			loader: fromPack || loader === NONE ? null : loader,
			loaderVersion: fromPack || loaderVersion === LATEST ? null : loaderVersion,
			port,
			memory,
			eula,
			publish: publish && canPublish,
			pack: pack || null
		});
		// Staying open with the reason on screen; closing would throw the whole form away.
		if (made) open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-[520px]">
		<Dialog.Header>
			<Dialog.Title>{t("host.create.title")}</Dialog.Title>
			<Dialog.Description>{t("host.create.hint")}</Dialog.Description>
		</Dialog.Header>

		<form id="new-server" onsubmit={create}>
			<div class="max-h-[58vh] space-y-4 overflow-y-auto py-2 pr-1">
				<!--
					The pack first, because it answers the two questions under it. A server built
					from a pack has its version and loader decided; one built from nothing asks.
				-->
				{#if fromPack}
					<div
						class="flex items-center gap-3 rounded-lg border border-primary/40 bg-primary/10 px-3.5 py-2.5"
					>
						<FileArchive class="size-4 shrink-0 text-primary" />
						<div class="min-w-0 flex-1">
							<p class="truncate text-sm font-medium" title={pack}>{packName}</p>
							<p class="text-xs text-muted-foreground">{t("host.packDecides")}</p>
						</div>
						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							class="text-muted-foreground"
							onclick={() => (pack = "")}
							aria-label={t("host.packClear")}
						>
							<X class="size-3.5" />
						</Button>
					</div>
				{:else}
					<Button type="button" variant="secondary" class="w-full" onclick={choosePack}>
						<FileArchive class="size-4" />
						{t("host.fromPack")}
					</Button>
				{/if}

				<div class="space-y-2">
					<label for="host-name" class="block text-sm font-medium">{t("host.name")}</label>
					<Input id="host-name" bind:value={name} placeholder={t("common.optional")} />
				</div>

				{#if !fromPack}
					<div class="grid grid-cols-2 gap-3">
						<div class="space-y-2">
							<span class="block text-sm font-medium">{t("instances.version")}</span>
							<Select.Root type="single" bind:value={minecraft}>
								<Select.Trigger aria-label={t("instances.version")}>
									<span class={minecraft ? "" : "text-muted-foreground"}>
										{minecraft || t("instances.versionPick")}
									</span>
								</Select.Trigger>
								<Select.Content>
									{#if launcher.versions.length === 0}
										<Select.Item
											value="__none"
											label={t("instances.versionsUnavailable")}
											disabled
										/>
									{/if}
									{#each launcher.versions as option (option)}
										<Select.Item value={option} label={option} />
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
					</div>

					{#if loader !== NONE}
						<div class="space-y-2">
							<span class="block text-sm font-medium">{t("instances.loaderVersion")}</span>
							<Select.Root type="single" bind:value={loaderVersion} disabled={loadingVersions}>
								<Select.Trigger aria-label={t("instances.loaderVersion")}>
									<span>
										{loadingVersions
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
				{/if}

				<!--
					The switch this whole tab exists for. A friend who has never heard of a mod
					loader adds the address, presses play, and the launcher fetches the pack from
					the server before the game starts.
				-->
				<label
					class="flex items-start gap-3 rounded-lg border px-3.5 py-3 transition-colors
					       {publishing ? 'border-ok/40 bg-ok/5' : 'border-border/70'}"
				>
					<Switch
						checked={publishing}
						onCheckedChange={(next) => (publish = next)}
						disabled={!canPublish}
						class="mt-0.5"
					/>
					<span class="min-w-0 flex-1">
						<span class="flex items-center gap-1.5 text-sm font-medium">
							<Zap class="size-3.5 {publishing ? 'text-ok' : 'text-muted-foreground'}" />
							{t("host.publishOption")}
						</span>
						<span class="mt-0.5 block text-xs text-muted-foreground">
							{canPublish ? t("host.publishOptionHint") : t("host.publishNeedsLoader")}
						</span>
					</span>
				</label>

				<!-- Mojang's condition, not ours, and it is the player's statement to make. -->
				<label
					class="flex items-start gap-3 rounded-lg border px-3.5 py-3 transition-colors
					       {eula ? 'border-border/70' : 'border-warn/40 bg-warn/5'}"
				>
					<Switch bind:checked={eula} class="mt-0.5" />
					<span class="min-w-0 flex-1">
						<span class="block text-sm font-medium">{t("host.eula")}</span>
						<span class="mt-0.5 block text-xs text-muted-foreground">
							{t("host.eulaHint")}
						</span>
					</span>
				</label>

				<div class="grid grid-cols-2 gap-3">
					<div class="space-y-2">
						<label for="host-port" class="block text-sm font-medium">{t("host.port")}</label>
						<NumberField id="host-port" bind:value={port} min={1024} max={65535} />
					</div>
					<div class="space-y-2">
						<label for="host-memory" class="block text-sm font-medium">{t("host.memory")}</label>
						<NumberField
							id="host-memory"
							bind:value={memory}
							min={1024}
							max={maxMemory}
							step={512}
						/>
					</div>
				</div>

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

		</form>
			<!--
				Outside the form, linked back to it by id. Dialog.Footer bleeds to the dialog's
				edges with a negative margin, which only lands right when it is a direct child
				of the content box - inside the form it renders wider than its own parent and
				pushes the dialog sideways.
			-->
			<Dialog.Footer>
				{#if !ready}
					<span class="mr-auto self-center text-xs text-muted-foreground">
						{!eula ? t("host.needsEula") : t("host.needsVersion")}
					</span>
				{/if}
				<Button type="button" variant="ghost" onclick={() => (open = false)}>
					{t("common.cancel")}
				</Button>
				<Button type="submit" form="new-server" class="cta-glow bg-cta text-cta-foreground hover:bg-cta/90" disabled={!ready || launcher.busyHosting}>
					<Sparkles class="size-4" />
					{t("host.build")}
				</Button>
			</Dialog.Footer>

	</Dialog.Content>
</Dialog.Root>
