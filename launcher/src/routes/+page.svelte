<script lang="ts">
	import { onMount } from "svelte";
	import { Plus, RefreshCw, TriangleAlert } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import TitleBar from "$lib/components/app/TitleBar.svelte";
	import ServerCard from "$lib/components/app/ServerCard.svelte";
	import ProgressOverlay from "$lib/components/app/ProgressOverlay.svelte";
	import SettingsDialog from "$lib/components/app/SettingsDialog.svelte";
	import ModBrowser from "$lib/components/app/ModBrowser.svelte";
	import { launcher } from "$lib/state.svelte";

	let settingsOpen = $state(false);
	let adding = $state(false);
	let address = $state("");
	let name = $state("");

	// onMount, not $effect: start() writes the same state it reads, and an effect that
	// tracks its own writes re-runs itself for as long as the window is open.
	onMount(() => {
		void launcher.start();
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		await launcher.add(name, address);
		address = "";
		name = "";
		adding = false;
	}
</script>

<div class="relative flex h-full flex-col">
	<TitleBar onsettings={() => (settingsOpen = true)} />

	{#if launcher.browsing}
		<ModBrowser />
	{:else}
	<main class="flex min-h-0 flex-1 flex-col px-5 pb-5 pt-4">
		<div class="mb-3 flex items-center gap-2">
			<h1 class="text-base font-semibold tracking-tight">Server</h1>
			<span class="text-xs text-muted-foreground">{launcher.servers.length}</span>

			<div class="flex-1"></div>

			<Button
				variant="ghost"
				size="icon"
				class="size-7 text-muted-foreground"
				onclick={() => launcher.probeAll()}
				aria-label="Neu prüfen"
			>
				<RefreshCw class="size-4" />
			</Button>
			<Button size="sm" class="h-8 text-xs" onclick={() => (adding = !adding)}>
				<Plus class="size-4" />
				Server
			</Button>
		</div>

		{#if adding}
			<form onsubmit={submit} class="mb-3 flex gap-2">
				<Input bind:value={address} placeholder="host:port" class="font-mono" autofocus />
				<Input bind:value={name} placeholder="Name (optional)" class="max-w-[180px]" />
				<Button type="submit" size="sm" class="h-9">Hinzufügen</Button>
			</form>
		{/if}

		{#if launcher.error}
			<div
				class="mb-3 flex items-start gap-2.5 rounded-lg border border-destructive/40 bg-destructive/10 px-3.5 py-2.5"
			>
				<TriangleAlert class="mt-0.5 size-4 shrink-0 text-destructive" />
				<p class="text-xs leading-relaxed text-destructive-foreground/90">{launcher.error}</p>
			</div>
		{/if}

		<!--
			A plain overflow container, not the library's scroll area: that one measures itself
			with a resize observer, and inside a conditional flex child it kept re-measuring
			until the renderer stopped answering. The scrollbar is styled in app.css anyway.
		-->
		<div class="-mx-1 min-h-0 flex-1 overflow-y-auto px-1">
			<div class="flex flex-col gap-2">
				{#each launcher.servers as server (server.id)}
					<ServerCard
						{server}
						status={launcher.status[server.id]}
						hubVersion="1.21.11"
						busy={launcher.playing === server.id}
						onplay={() => launcher.play(server)}
						onmods={() => launcher.openMods(server)}
						onremove={() => launcher.remove(server.id)}
					/>
				{/each}

				{#if launcher.servers.length === 0}
					<div
						class="rounded-lg border border-dashed border-border/70 px-4 py-10 text-center"
					>
						<p class="text-sm text-muted-foreground">Noch kein Server</p>
						<p class="mt-1 text-xs text-muted-foreground/70">
							Adresse eintragen, den Rest holt sich der Client vom Server.
						</p>
					</div>
				{/if}
			</div>
		</div>
	</main>
	{/if}

	{#if launcher.progress}
		<ProgressOverlay job={launcher.progress} />
	{/if}

	<SettingsDialog bind:open={settingsOpen} />
</div>
