<script lang="ts">
	import * as Dialog from "$lib/components/ui/dialog";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import { launcher } from "$lib/state.svelte";
	import type { Settings } from "$lib/types";

	let { open = $bindable(false) }: { open: boolean } = $props();

	/** Edited locally, written on save - so cancelling really cancels. */
	let draft = $state<Settings>({ ...launcher.settings });

	$effect(() => {
		if (open) draft = { ...launcher.settings };
	});

	const presets = [
		{ value: 0, label: "Auto" },
		{ value: 3072, label: "3 GB" },
		{ value: 4096, label: "4 GB" },
		{ value: 6144, label: "6 GB" },
		{ value: 8192, label: "8 GB" }
	];

	async function save() {
		await launcher.saveSettings(draft);
		open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-[420px]">
		<Dialog.Header>
			<Dialog.Title>Einstellungen</Dialog.Title>
		</Dialog.Header>

		<div class="space-y-5 py-2">
			<div class="space-y-2">
				<div class="flex items-baseline justify-between">
					<label for="memory" class="text-sm font-medium">Arbeitsspeicher</label>
					<span class="text-xs text-muted-foreground">
						Auto richtet sich nach der Packgröße
					</span>
				</div>
				<div id="memory" class="flex gap-1.5">
					{#each presets as preset (preset.value)}
						<Button
							variant={draft.memory === preset.value ? "default" : "secondary"}
							size="sm"
							class="h-9 flex-1 text-xs"
							onclick={() => (draft.memory = preset.value)}
						>
							{preset.label}
						</Button>
					{/each}
				</div>
			</div>

			<div class="space-y-2">
				<label for="port" class="text-sm font-medium">Manifest-Port</label>
				<Input id="port" type="number" bind:value={draft.manifest_port} class="font-mono" />
				<p class="text-xs text-muted-foreground">
					Der Port, auf dem das Server-Mod veröffentlicht. Muss zu
					<code class="font-mono">config/unifiedmc.properties</code> auf dem Server passen.
				</p>
			</div>

			<div class="space-y-2">
				<label for="name" class="text-sm font-medium">Name ohne Anmeldung</label>
				<Input id="name" bind:value={draft.offline_name} />
				<p class="text-xs text-muted-foreground">
					Gilt nur, solange keine Sitzung vorliegt. Online-Mode-Server lehnen ihn ab.
				</p>
			</div>
		</div>

		<Dialog.Footer>
			<Button variant="ghost" onclick={() => (open = false)}>Abbrechen</Button>
			<Button onclick={save}>Speichern</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
