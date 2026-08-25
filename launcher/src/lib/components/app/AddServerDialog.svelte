<script lang="ts">
	import { TriangleAlert } from "@lucide/svelte";
	import * as Dialog from "$lib/components/ui/dialog";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";

	let { open = $bindable(false) }: { open: boolean } = $props();

	let address = $state("");
	let name = $state("");
	let touched = $state(false);

	$effect(() => {
		if (!open) return;
		address = "";
		name = "";
		touched = false;
		launcher.error = null;
	});

	/**
	 * host, host:port or an IPv4. No DNS check: a server that is down right now is still one you
	 * want in the list.
	 */
	const valid = $derived.by(() => {
		const raw = address.trim();
		if (!raw) return false;
		const [host, port, ...rest] = raw.split(":");
		if (rest.length) return false;
		if (port !== undefined) {
			if (!/^\d{1,5}$/.test(port)) return false;
			const number = Number(port);
			if (number < 1 || number > 65535) return false;
		}
		return /^[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*$/.test(
			host
		);
	});

	async function add(event: SubmitEvent) {
		event.preventDefault();
		touched = true;
		if (!valid) return;
		await launcher.add(name, address.trim());
		// Staying open with the reason on screen; closing would hide why nothing was added.
		if (launcher.error) return;
		open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-[460px]">
		<Dialog.Header>
			<Dialog.Title>{t("servers.add.title")}</Dialog.Title>
		</Dialog.Header>

		<form onsubmit={add}>
			<div class="space-y-4 py-2">
				<div class="space-y-2">
					<label for="add-server-address" class="block text-sm font-medium">
						{t("servers.address")}
					</label>
					<Input
						id="add-server-address"
						bind:value={address}
						aria-describedby={touched && !valid ? "add-server-address-error" : undefined}
						placeholder="host:port"
						class="font-mono"
						aria-invalid={touched && !valid}
						onblur={() => (touched = true)}
						autofocus
					/>
					{#if touched && !valid}
						<p id="add-server-address-error" class="text-xs break-words text-destructive">
							{t("servers.addressInvalid")}
						</p>
					{/if}
				</div>

				<div class="space-y-2">
					<label for="add-server-name" class="block text-sm font-medium">{t("servers.name")}</label>
					<Input id="add-server-name" bind:value={name} placeholder={t("common.optional")} />
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

			<Dialog.Footer>
				<Button type="button" variant="ghost" onclick={() => (open = false)}>
					{t("common.cancel")}
				</Button>
				<Button type="submit" disabled={!address.trim()}>{t("common.add")}</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
