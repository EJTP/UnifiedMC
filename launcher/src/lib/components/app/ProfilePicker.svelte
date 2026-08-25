<script lang="ts">
	import { Plus } from "@lucide/svelte";
	import * as Dialog from "$lib/components/ui/dialog";
	import { Button } from "$lib/components/ui/button";
	import CreateInstanceDialog from "./CreateInstanceDialog.svelte";
	import * as Select from "$lib/components/ui/select";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";
	import { cn } from "$lib/utils";
	import type { SavedServer } from "$lib/types";

	/**
	 * Which server is being asked about; null closes the dialog. The open state lives with the
	 * caller: a one-way prop plus a local write can never be reopened for the same row.
	 */
	let {
		server,
		onchoose,
		oncancel
	}: {
		server: SavedServer | null;
		onchoose: (instanceId: string | null) => void;
		oncancel: () => void;
	} = $props();

	const open = $derived(server !== null);

	/** null means the server's own setup, which is also what every fresh opening starts on. */
	let chosen = $state<string | null>(null);
	let creating = $state(false);

	$effect(() => {
		if (open) chosen = null;
		// the caller can drop the server from under us; a stranded create dialog would remain
		else creating = false;
	});

	const status = $derived(server ? (launcher.status[server.id] ?? null) : null);
	const manifest = $derived(status?.manifest ?? null);

	/** What the server was detected as, or what the player set for it when detection failed. */
	const version = $derived(manifest?.minecraft ?? server?.minecraft ?? null);

	/** Vanilla and Paper announce nothing here, and that is the case the matching hinges on. */
	const serverLoader = $derived(manifest?.loader?.type ?? server?.loader ?? null);

	const serverFacts = $derived.by(() => {
		if (!status) return t("status.checking");
		if (!status.online) return t("status.offline");
		if (!version) return t("profile.serverDefaultHint");
		return [version, serverLoader ?? t("instances.noMods")].join("  ·  ");
	});

	/**
	 * A server that runs no loader cares only about the version - client-side mods need no server
	 * side. One that does would turn away an instance built on another.
	 */
	const matching = $derived(
		version
			? launcher.instances.filter(
					(instance) =>
						instance.minecraft === version &&
						(!serverLoader || (instance.loader ?? null) === serverLoader)
				)
			: []
	);

	/** A pack prescribes the version. Anything else is a version question, asked here. */
	const prescribed = $derived((manifest?.mods.length ?? 0) > 0);

	/** Sentinel, never "": bits-ui reads the empty string as "no value" and locks up on it. */
	const AUTO = "auto";
	let wanted = $state(AUTO);

	$effect(() => {
		if (open) wanted = server?.minecraft ?? AUTO;
	});

	/** Written straight through: play() reads the choice back off the server. */
	function chooseVersion(next: string) {
		wanted = next;
		if (server) void launcher.configure(server, next === AUTO ? null : next, server.loader);
	}

	/** A freshly created profile stays selectable even when it does not match. */
	const rows = $derived.by(() => {
		if (!chosen || matching.some((instance) => instance.id === chosen)) return matching;
		const extra = launcher.instances.find((instance) => instance.id === chosen);
		return extra ? [...matching, extra] : matching;
	});

	const row =
		"w-full cursor-pointer rounded-lg border border-border/70 bg-card px-4 py-3 text-left" +
		" transition-colors duration-200 hover:bg-accent/40 focus-visible:outline-none" +
		" focus-visible:ring-3 focus-visible:ring-ring/50";
	const picked = "border-primary/60 bg-primary/15";

	function play() {
		onchoose(chosen);
	}
</script>

<Dialog.Root {open} onOpenChange={(next) => !next && oncancel()}>
	<Dialog.Content class="sm:max-w-[460px]">
		<Dialog.Header>
			<Dialog.Title>{t("profile.title")}</Dialog.Title>
			<Dialog.Description class="truncate">{server?.name ?? ""}</Dialog.Description>
		</Dialog.Header>

		<!--
			No scroll box of its own: the dialog already stops at the window and scrolls, and its
			footer is sticky, so a second cap here only shortened the list to well under the space
			it had - the version card and half the profiles sat behind a hairline scrollbar with an
			empty dialog underneath. -mx-1/px-1 stays, for the focus rings of the rows.
		-->
		<div class="-mx-1 space-y-2 px-1 py-2">
			<button
				type="button"
				aria-pressed={chosen === null}
				class={cn(row, chosen === null && picked)}
				onclick={() => (chosen = null)}
			>
				<span class="block truncate text-sm font-medium">{t("profile.serverDefault")}</span>
				<span class="mt-0.5 block truncate text-xs text-muted-foreground">{serverFacts}</span>
			</button>

			{#if !prescribed && chosen === null}
				<div class="rounded-lg border border-border/70 bg-card/50 px-4 py-3">
					<span class="mb-2 block text-xs font-medium text-muted-foreground">
						{t("profile.version")}
					</span>
					<Select.Root type="single" value={wanted} onValueChange={chooseVersion}>
						<Select.Trigger aria-label={t("profile.version")}>
							<span class={wanted === AUTO ? "" : "font-mono"}>
								{wanted === AUTO ? t("setup.versionAuto") : wanted}
							</span>
						</Select.Trigger>
						<Select.Content>
							<Select.Item
								value={AUTO}
								label={t("setup.versionAuto")}
								hint={version ? t("setup.versionDetected", { version }) : undefined}
							/>
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
					<p class="mt-2 text-xs leading-relaxed text-muted-foreground">
						{t("profile.versionHint")}
					</p>
				</div>
			{/if}

			{#each rows as instance (instance.id)}
				<button
					type="button"
					aria-pressed={chosen === instance.id}
					class={cn(row, chosen === instance.id && picked)}
					onclick={() => (chosen = instance.id)}
				>
					<span class="block truncate text-sm font-medium">{instance.name}</span>
					<span class="mt-0.5 block truncate text-xs text-muted-foreground">
						{[instance.minecraft, instance.loader ?? t("instances.noMods")].join("  ·  ")}
					</span>
				</button>
			{/each}

			{#if rows.length === 0}
				<!-- No px of its own: it has to start on the same line as the rows above and below it. -->
				<p class="py-1 text-xs break-words text-muted-foreground">{t("profile.none")}</p>
			{/if}

			<button
				type="button"
				class={cn(row, "border-dashed")}
				onclick={() => {
					// the version picker needs the list, and this is the only way in from here
					void launcher.loadVersions();
					creating = true;
				}}
			>
				<span class="flex min-w-0 items-center gap-2 text-sm font-medium">
					<Plus class="size-4 shrink-0 text-muted-foreground" />
					<span class="min-w-0 truncate">{t("profile.create")}</span>
				</span>
				<span class="mt-0.5 block truncate text-xs text-muted-foreground">
					{t("profile.createHint")}
				</span>
			</button>
		</div>

		<Dialog.Footer>
			<Button variant="ghost" onclick={oncancel}>{t("common.cancel")}</Button>
			<!--
				Nothing to launch: with no version detected and none chosen, "as the server has it"
				names a setup that does not exist yet - the way out is a profile, or the gear.
			-->
			<Button
				class="bg-cta text-cta-foreground hover:bg-cta/90"
				disabled={chosen === null && !version}
				onclick={play}
			>
				{t("profile.play")}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<CreateInstanceDialog
	bind:open={creating}
	minecraft={version}
	loader={serverLoader}
	oncreated={(id) => (chosen = id)}
/>
