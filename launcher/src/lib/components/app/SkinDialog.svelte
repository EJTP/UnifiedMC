<script lang="ts">
	import { TriangleAlert } from "@lucide/svelte";
	import * as Dialog from "$lib/components/ui/dialog";
	import { Button } from "$lib/components/ui/button";
	import { Switch } from "$lib/components/ui/switch";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";

	let { open = $bindable(false) }: { open: boolean } = $props();

	/** The whole data: URL, so the same string draws the preview and carries the bytes. */
	let picked = $state<string | null>(null);
	let filename = $state("");
	let slim = $state(false);
	let busy = $state(false);
	let failure = $state<string | null>(null);
	let done = $state(false);

	/** An offline profile has no token, so there is nothing to send and nothing to offer. */
	const microsoft = $derived(launcher.session?.kind === "microsoft");

	$effect(() => {
		if (!open) return;
		picked = null;
		filename = "";
		failure = null;
		done = false;
		busy = false;
	});

	/** The webview reads the file itself; a file dialog plugin would only hand Rust a path. */
	function choose(event: Event) {
		const file = (event.currentTarget as HTMLInputElement).files?.[0];
		failure = null;
		done = false;
		filename = file?.name ?? "";
		if (!file) {
			picked = null;
			return;
		}
		// Rust refuses anything over this too, but only after the webview has read the whole
		// file into a base64 string - so a 500 MB pick would freeze this window to be told no.
		if (file.size > 256 * 1024) {
			picked = null;
			failure = t("error.skinTooBig");
			return;
		}
		const reader = new FileReader();
		reader.onload = () => (picked = String(reader.result));
		reader.onerror = () => {
			picked = null;
			failure = t("skin.unreadable");
		};
		reader.readAsDataURL(file);
	}

	async function apply() {
		if (!picked) return;
		busy = true;
		// Everything after the comma is the base64 Rust wants; the prefix is the browser's.
		failure = await launcher.setSkin(picked.slice(picked.indexOf(",") + 1), slim);
		done = !failure;
		busy = false;
	}

	async function reset() {
		busy = true;
		failure = await launcher.resetSkin();
		if (!failure) {
			picked = null;
			filename = "";
			done = true;
		}
		busy = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-[440px]">
		<Dialog.Header>
			<Dialog.Title>{t("skin.title")}</Dialog.Title>
			<Dialog.Description>
				{microsoft ? t("skin.description") : t("skin.needsMicrosoft")}
			</Dialog.Description>
		</Dialog.Header>

		{#if microsoft}
			<div class="space-y-4 py-1">
				<div class="flex items-center gap-4">
					<div class="flex flex-col items-center gap-1.5">
						{#if launcher.playerHead}
							<!-- pixelated: a face is eight pixels wide, and smoothing it is mush -->
							<img
								src={launcher.playerHead}
								alt=""
								class="size-14 rounded-md [image-rendering:pixelated]"
							/>
						{:else}
							<div class="size-14 rounded-md bg-muted"></div>
						{/if}
						<span class="text-[0.7rem] text-muted-foreground">{t("skin.current")}</span>
					</div>

					<div class="min-w-0 flex-1">
						<p class="truncate text-sm">{launcher.session?.name}</p>
						<p class="truncate text-xs text-muted-foreground" title={filename}>
							{filename || t("skin.noFile")}
						</p>
					</div>

					{#if picked}
						<div class="flex flex-col items-center gap-1.5">
							<img
								src={picked}
								alt=""
								class="size-14 rounded-md border border-border/70 bg-muted object-contain [image-rendering:pixelated]"
							/>
							<span class="text-[0.7rem] text-muted-foreground">{t("skin.preview")}</span>
						</div>
					{/if}
				</div>

				<div class="space-y-2">
					<label for="skin-file" class="block text-sm font-medium">{t("skin.file")}</label>
					<!--
						The plain control, styled: accept filters the picker to PNGs, and whether the
						file is really a 64x64 skin is Rust's answer to give, not this dialog's guess.
					-->
					<input
						id="skin-file"
						type="file"
						accept="image/png"
						onchange={choose}
						class="block w-full cursor-pointer overflow-hidden rounded-lg border border-input bg-input/40
						       text-sm text-muted-foreground outline-none focus-visible:border-ring
						       focus-visible:ring-3 focus-visible:ring-ring/50
						       file:mr-3 file:cursor-pointer file:border-0 file:bg-secondary file:px-3 file:py-2
						       file:text-sm file:text-foreground"
					/>
					<p class="text-xs text-muted-foreground">{t("skin.fileHint")}</p>
				</div>

				<div class="flex items-start justify-between gap-4">
					<div class="min-w-0 space-y-1">
						<span id="skin-slim" class="block text-sm font-medium">{t("skin.slim")}</span>
						<p class="text-xs text-muted-foreground">{t("skin.slimHint")}</p>
					</div>
					<Switch
						aria-labelledby="skin-slim"
						checked={slim}
						onCheckedChange={(next) => (slim = next)}
					/>
				</div>

				{#if failure}
					<div
						class="flex items-start gap-2.5 rounded-lg border border-destructive/40
						       bg-destructive/10 px-3.5 py-2.5"
					>
						<TriangleAlert class="mt-0.5 size-4 shrink-0 text-destructive" />
						<p class="min-w-0 text-xs leading-relaxed break-words text-destructive-foreground/90">
							{failure}
						</p>
					</div>
				{:else if done}
					<p class="text-xs break-words text-ok">{t("skin.done")}</p>
				{/if}
			</div>

			<Dialog.Footer>
				<Button variant="ghost" class="sm:mr-auto" disabled={busy} onclick={reset}>
					{t("skin.reset")}
				</Button>
				<!--
					Signing out belongs where the account is, and this dialog is the only place
					the account is more than a name in a corner.
				-->
				<Button
					variant="ghost"
					class="text-muted-foreground hover:text-destructive"
					disabled={busy}
					onclick={async () => {
						await launcher.signOut();
						open = false;
					}}
				>
					{t("signIn.signOut")}
				</Button>
				<Button variant="ghost" onclick={() => (open = false)}>{t("common.close")}</Button>
				<Button disabled={busy || !picked} onclick={apply}>{t("skin.apply")}</Button>
			</Dialog.Footer>
		{:else}
			<Dialog.Footer>
				<Button variant="ghost" onclick={() => (open = false)}>{t("common.close")}</Button>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>
