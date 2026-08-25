<script lang="ts">
	import { onMount } from "svelte";
	import { Plus, RefreshCw, TriangleAlert, X } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import Sidebar from "$lib/components/app/Sidebar.svelte";
	import TitleBar from "$lib/components/app/TitleBar.svelte";
	import ServerCard from "$lib/components/app/ServerCard.svelte";
	import InstanceCard from "$lib/components/app/InstanceCard.svelte";
	import ProgressOverlay from "$lib/components/app/ProgressOverlay.svelte";
	import SettingsDialog from "$lib/components/app/SettingsDialog.svelte";
	import ModBrowser from "$lib/components/app/ModBrowser.svelte";
	import CreateInstanceDialog from "$lib/components/app/CreateInstanceDialog.svelte";
	import AddServerDialog from "$lib/components/app/AddServerDialog.svelte";
	import ProfilePicker from "$lib/components/app/ProfilePicker.svelte";
	import ServerSetupDialog from "$lib/components/app/ServerSetupDialog.svelte";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";
	import type { SavedServer } from "$lib/types";

	let settingsOpen = $state(false);
	let addingServer = $state(false);
	let addingInstance = $state(false);
	/** The server whose setup is open, if one is. */
	let setup = $state<SavedServer | null>(null);

	// onMount, not $effect: start() writes the same state it reads, and an effect that tracks
	// its own writes re-runs itself for as long as the window is open.
	onMount(() => {
		void launcher.start();
	});

	const servers = $derived(launcher.view === "servers");

	function add() {
		if (servers) {
			addingServer = true;
		} else {
			// the picker needs the list; loading it here means it is there when the dialog opens
			void launcher.loadVersions();
			addingInstance = true;
		}
	}

	function openSetup(server: SavedServer) {
		// same reason as above: the version picker is only useful once the list has arrived
		void launcher.loadVersions();
		setup = server;
	}
</script>

<!-- The system decorations are off, so the window's top edge is ours to draw and to drag. -->
<div class="flex h-full flex-col">
	<TitleBar />

	<div class="relative flex min-h-0 flex-1">
		<Sidebar onsettings={() => (settingsOpen = true)} />

	<div class="flex min-w-0 flex-1 flex-col">
		{#if launcher.browsing}
			<ModBrowser />
		{:else}
			<!--
				h-11 on the header, the same strip the sidebar spends on the logo: otherwise the
				title sits lower than the word next to it and the two columns start on different
				lines.
			-->
			<main class="flex min-h-0 flex-1 flex-col px-6 pb-6">
				<div class="flex h-11 shrink-0 items-center gap-3">
					<h1 class="min-w-0 truncate text-lg font-semibold tracking-tight">
						{servers ? t("servers.title") : t("instances.title")}
					</h1>

					<div class="flex-1"></div>

					{#if servers}
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
						{servers ? t("servers.addAction") : t("instances.addAction")}
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
						{#if servers}
							{#each launcher.servers as server (server.id)}
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

							<!--
								"Nothing here" is only true once the list has arrived; before that the same
								empty box would be a lie the player reads for as long as the disk takes.
							-->
							{#if launcher.servers.length === 0}
								<div class="rounded-lg border border-dashed border-border/70 px-4 py-12 text-center">
									{#if launcher.booted}
										<p class="text-sm text-muted-foreground">{t("servers.empty.title")}</p>
										<p class="mx-auto mt-1.5 max-w-md text-xs text-muted-foreground/70">
											{t("servers.empty.hint")}
										</p>
									{:else}
										<p class="text-sm text-muted-foreground">{t("servers.loading")}</p>
									{/if}
								</div>
							{/if}
						{:else}
							{#each launcher.instances as instance (instance.id)}
								<InstanceCard
									{instance}
									busy={launcher.playing === instance.id}
									onplay={() => launcher.playInstance(instance)}
									onmods={() => launcher.openInstanceMods(instance)}
									onremove={() => launcher.removeInstance(instance.id)}
								/>
							{/each}

							{#if launcher.instances.length === 0}
								<div class="rounded-lg border border-dashed border-border/70 px-4 py-12 text-center">
									{#if launcher.booted}
										<p class="text-sm text-muted-foreground">{t("instances.empty.title")}</p>
										<p class="mx-auto mt-1.5 max-w-md text-xs text-muted-foreground/70">
											{t("instances.empty.hint")}
										</p>
									{:else}
										<p class="text-sm text-muted-foreground">{t("instances.loading")}</p>
									{/if}
								</div>
							{/if}
						{/if}
					</div>
				</div>
			</main>
		{/if}
	</div>

	{#if launcher.progress}
		<ProgressOverlay job={launcher.progress} />
	{/if}

	<SettingsDialog bind:open={settingsOpen} />
	<AddServerDialog bind:open={addingServer} />
	<CreateInstanceDialog bind:open={addingInstance} />
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
