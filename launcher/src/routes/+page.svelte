<script lang="ts">
	import { onMount } from "svelte";
	import { HardDrive, Plus, RefreshCw, Search, TriangleAlert, X } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import Sidebar from "$lib/components/app/Sidebar.svelte";
	import TitleBar from "$lib/components/app/TitleBar.svelte";
	import ServerCard from "$lib/components/app/ServerCard.svelte";
	import InstanceCard from "$lib/components/app/InstanceCard.svelte";
	import HostCard from "$lib/components/app/HostCard.svelte";
	import HostConsole from "$lib/components/app/HostConsole.svelte";
	import ProgressOverlay from "$lib/components/app/ProgressOverlay.svelte";
	import SettingsDialog from "$lib/components/app/SettingsDialog.svelte";
	import ModBrowser from "$lib/components/app/ModBrowser.svelte";
	import CreateInstanceDialog from "$lib/components/app/CreateInstanceDialog.svelte";
	import CreateHostDialog from "$lib/components/app/CreateHostDialog.svelte";
	import AddServerDialog from "$lib/components/app/AddServerDialog.svelte";
	import ProfilePicker from "$lib/components/app/ProfilePicker.svelte";
	import ServerSetupDialog from "$lib/components/app/ServerSetupDialog.svelte";
	import { launcher } from "$lib/state.svelte";
	import { byLastPlayed } from "$lib/played";
	import { t } from "$lib/i18n.svelte";
	import type { SavedServer } from "$lib/types";

	let settingsOpen = $state(false);
	let addingServer = $state(false);
	let addingInstance = $state(false);
	let addingHost = $state(false);
	/** The server whose setup is open, if one is. */
	let setup = $state<SavedServer | null>(null);

	/**
	 * The header's filter box, when the header is showing one. Typed as the element the shadcn
	 * Input binds back - HTMLElement, not HTMLInputElement - so the shortcut narrows it instead.
	 */
	let filterBox = $state<HTMLElement | null>(null);

	/** What to call the modifier in the tooltip. Nothing else in the window asks which platform. */
	const modKey = $derived(navigator.userAgent.includes("Mac") ? "⌘" : t("common.ctrlKey"));

	// onMount, not $effect: start() writes the same state it reads, and an effect that tracks
	// its own writes re-runs itself for as long as the window is open.
	onMount(() => {
		void launcher.start();
	});

	const view = $derived(launcher.view);

	/** Matched against everything a row shows, so what is on screen is what is searched. */
	function matches(...fields: (string | null | undefined)[]): boolean {
		const needle = launcher.filter.trim().toLowerCase();
		if (!needle) return true;
		return fields.some((field) => field?.toLowerCase().includes(needle));
	}

	// Most recently played first. There is no hand-arranged order to preserve, so the one the
	// player would have to reconstruct by memory is the one worth defaulting to.
	const servers = $derived(
		byLastPlayed(
			launcher.servers.filter((server) => matches(server.name, server.address)),
			(server) => `server:${server.address}`,
			launcher.playtime
		)
	);
	const instances = $derived(
		byLastPlayed(
			launcher.instances.filter((entry) => matches(entry.name, entry.minecraft, entry.loader)),
			(entry) => `instance:${entry.id}`,
			launcher.playtime
		)
	);
	const hosts = $derived(
		launcher.hosts.filter((server) =>
			matches(server.name, server.minecraft, server.loader, server.source, server.address)
		)
	);

	/** How many rows exist before the filter, so "no results" and "nothing here" stay distinct. */
	const total = $derived(
		view === "servers"
			? launcher.servers.length
			: view === "instances"
				? launcher.instances.length
				: launcher.hosts.length
	);
	const shown = $derived(
		view === "servers" ? servers.length : view === "instances" ? instances.length : hosts.length
	);

	const title = $derived(
		view === "servers"
			? t("servers.title")
			: view === "instances"
				? t("instances.title")
				: t("host.title")
	);

	const addLabel = $derived(
		view === "servers"
			? t("servers.addAction")
			: view === "instances"
				? t("instances.addAction")
				: t("host.addAction")
	);

	/**
	 * The window is drawn without decorations, so there is no menu bar to hang these on and the
	 * window itself has to listen. An open dialog is looked for in the DOM rather than through the
	 * flags above: bits-ui portals every one of them to the same slot, so a dialog cannot forget to
	 * declare itself here - and Escape stays bits-ui's to handle while one is up. A progress
	 * overlay is not a dialog but covers the same ground, and the header it hides must not be
	 * reachable by keyboard while the mouse cannot get at it.
	 */
	function shortcut(event: KeyboardEvent) {
		if (launcher.progress || launcher.closing) return;
		if (document.querySelector('[data-slot="dialog-content"]')) return;
		// Cmd on macOS, Ctrl everywhere else; no chord in the app wants to tell the two apart.
		const held = event.metaKey || event.ctrlKey;
		if (held && event.key.toLowerCase() === "f" && filterBox instanceof HTMLInputElement) {
			// select, not focus: a second press is somebody about to replace what they typed.
			event.preventDefault();
			filterBox.select();
		} else if (held && event.key === ",") {
			event.preventDefault();
			settingsOpen = true;
		} else if (event.key === "Escape" && launcher.filter) {
			// Not conditional on the box existing: the list can drop below four rows while a filter
			// is still typed, which unmounts both the box and the button and leaves this the only
			// way back to the whole list.
			// Escape typed into another box belongs to that box - the console's command line is
			// right under the hosting list, and the filter is not what it means to empty.
			const active = document.activeElement;
			if (active !== filterBox && active instanceof HTMLInputElement) return;
			launcher.filter = "";
		}
	}

	function add() {
		if (view === "servers") {
			addingServer = true;
			return;
		}
		// Both pickers need Mojang's version list; asking here means it is there on open.
		void launcher.loadVersions();
		if (view === "instances") {
			addingInstance = true;
		} else {
			addingHost = true;
		}
	}

	function openSetup(server: SavedServer) {
		// same reason as above: the version picker is only useful once the list has arrived
		void launcher.loadVersions();
		setup = server;
	}
</script>

<svelte:window onkeydown={shortcut} />

<!-- The system decorations are off, so the window's top edge is ours to draw and to drag. -->
<div class="ambient flex h-full flex-col">
	<TitleBar />

	<div class="relative flex min-h-0 flex-1">
		<Sidebar onsettings={() => (settingsOpen = true)} />

		<div class="flex min-w-0 flex-1 flex-col">
			{#if launcher.browsing}
				<ModBrowser />
			{:else}
				<!--
					h-11 on the header, the same strip the sidebar spends on its first row: otherwise
					the title sits lower than the word next to it and the two columns start on
					different lines.
				-->
				<main class="flex min-h-0 flex-1 flex-col px-6 pb-6">
					<div class="flex h-11 shrink-0 items-center gap-3">
						<h1 class="min-w-0 shrink-0 truncate text-lg font-semibold tracking-tight">
							{title}
						</h1>

						<!--
							The search box only once there is enough to search. Below four rows the
							whole list is on screen already, and a filter can only hide part of it.
						-->
						{#if total > 3}
							<div class="relative ml-2 max-w-64 min-w-0 flex-1">
								<Search
									class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
								/>
								<!-- The tooltip carries the shortcut, since there is no menu to read it off. -->
								<Input
									bind:ref={filterBox}
									bind:value={launcher.filter}
									placeholder={t("common.filter")}
									aria-label={t("common.filter")}
									title={t("common.filterHint", { key: modKey })}
									class="h-8 pl-8 text-xs"
								/>
							</div>
						{/if}

						<div class="flex-1"></div>

						{#if view === "servers"}
							<Button
								variant="ghost"
								size="icon-lg"
								class="text-muted-foreground"
								onclick={() => launcher.probeAll()}
								aria-label={t("servers.refresh")}
							>
								<RefreshCw class="size-4" />
							</Button>
						{/if}

						<!-- size lg, not sm at h-9: sm rounds its corners tighter than the icon button beside it -->
						<Button size="lg" onclick={add}>
							<Plus class="size-4" />
							{addLabel}
						</Button>
					</div>

					{#if launcher.error}
						<div
							class="mt-3 flex items-start gap-2.5 rounded-lg border border-destructive/40
							       bg-destructive/10 px-3.5 py-2.5"
						>
							<TriangleAlert class="mt-0.5 size-4 shrink-0 text-destructive" />
							<p
								class="min-w-0 flex-1 text-xs leading-relaxed break-words whitespace-pre-line text-destructive-foreground/90"
							>
								{launcher.error}
							</p>
							<Button
								variant="ghost"
								size="icon-sm"
								class="-mt-0.5 -mr-1 shrink-0 text-destructive"
								onclick={() => (launcher.error = null)}
								aria-label={t("common.close")}
							>
								<X class="size-3.5" />
							</Button>
						</div>
					{/if}

					<!--
						A plain overflow container, not the library's scroll area: that one measures
						itself with a resize observer and, inside a conditional flex child, kept
						re-measuring until the renderer stopped answering.
					-->
					<div class="-mx-1 mt-3 min-h-0 flex-1 overflow-y-auto px-1">
						<div class="flex flex-col gap-2">
							{#if view === "servers"}
								{#each servers as server (server.id)}
									<ServerCard
										{server}
										status={launcher.status[server.id]}
										busy={launcher.playing === server.id}
										onplay={() => launcher.askThenPlay(server)}
										onmods={() => launcher.openMods(server)}
										onsetup={() => openSetup(server)}
										onremove={() => launcher.remove(server.id)}
									/>
								{/each}
							{:else if view === "instances"}
								{#each instances as instance (instance.id)}
									<InstanceCard
										{instance}
										busy={launcher.playing === instance.id}
										onplay={() => launcher.playInstance(instance)}
										onmods={() => launcher.openInstanceMods(instance)}
										onremove={() => launcher.removeInstance(instance.id)}
									/>
								{/each}
							{:else}
								{#each hosts as server (server.id)}
									<HostCard {server} />
								{/each}
								<!--
									The console under the list rather than over it: it is watched while
									the row above it is used, and a modal would take the stop button away.
								-->
								<HostConsole />
							{/if}

							<!--
								Three states, not two. "Nothing here" is only true once the list has
								arrived; before that the same empty box is a lie the player reads for as
								long as the disk takes. And a filter that matched nothing is a fourth
								thing again - the list is not empty, the search is.
							-->
							{#if shown === 0}
								<div
									class="surface flex flex-col items-center px-4 py-14 text-center"
								>
									{#if !launcher.booted}
										<p class="text-sm text-muted-foreground">{t("common.loading")}</p>
									{:else if total > 0}
										<p class="text-sm text-muted-foreground">
											{t("common.noMatches", { query: launcher.filter })}
										</p>
										<Button
											variant="ghost"
											size="sm"
											class="mt-2"
											onclick={() => (launcher.filter = "")}
										>
											{t("common.clearFilter")}
										</Button>
									{:else if view === "hosting"}
										<!--
											The one empty state worth designing: nobody arrives knowing a
											launcher can run the server too, so this has to say what the
											tab is for rather than that it is empty.
										-->
										<div
											class="flex size-12 items-center justify-center rounded-xl bg-primary/15"
										>
											<HardDrive class="size-5 text-primary" />
										</div>
										<p class="mt-3 text-sm font-medium">{t("host.empty.title")}</p>
										<p class="mx-auto mt-1.5 max-w-md text-xs leading-relaxed text-muted-foreground">
											{t("host.empty.hint")}
										</p>
										<Button size="lg" class="cta-glow bg-cta text-cta-foreground hover:bg-cta/90 mt-4" onclick={add}>
											<Plus class="size-4" />
											{t("host.addAction")}
										</Button>
									{:else}
										<p class="text-sm text-muted-foreground">
											{view === "servers" ? t("servers.empty.title") : t("instances.empty.title")}
										</p>
										<p class="mx-auto mt-1.5 max-w-md text-xs text-muted-foreground-dim">
											{view === "servers" ? t("servers.empty.hint") : t("instances.empty.hint")}
										</p>
									{/if}
								</div>
							{/if}
						</div>
					</div>
				</main>
			{/if}
		</div>

		<!--
			Shutdown beats every other overlay: whatever else was on screen, the window is going
			away and this is the only thing still true about it.
		-->
		{#if launcher.closing}
			<ProgressOverlay
				job={{ phase: t("host.closing"), detail: "", done: 0, total: 0 }}
			/>
		{:else if launcher.progress}
			<ProgressOverlay job={launcher.progress} oncancel={() => launcher.cancel()} />
		{/if}

		<SettingsDialog bind:open={settingsOpen} />
		<AddServerDialog bind:open={addingServer} />
		<CreateInstanceDialog bind:open={addingInstance} />
		<CreateHostDialog bind:open={addingHost} />
		<ServerSetupDialog server={setup} onclose={() => (setup = null)} />

		<!--
			Asked whenever the server prescribes nothing: a Vanilla or Paper server decides no pack,
			so which instance to bring is the player's answer to give - and creating one is reachable
			from that dialog and nowhere else.
		-->
		<ProfilePicker
			server={launcher.choosing}
			onchoose={(instance) => {
				if (launcher.choosing) void launcher.play(launcher.choosing, instance);
			}}
			oncancel={() => (launcher.choosing = null)}
		/>
	</div>
</div>
