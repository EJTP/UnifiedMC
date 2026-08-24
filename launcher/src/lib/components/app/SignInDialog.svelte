<script lang="ts">
	import { Check, Copy, ExternalLink, Loader2, TriangleAlert } from "@lucide/svelte";
	import * as Dialog from "$lib/components/ui/dialog";
	import { Button } from "$lib/components/ui/button";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";

	/**
	 * Open while there is something for the player to do, or something to tell them. The state
	 * lives on the launcher rather than here: the sign-in is started from the sidebar and can
	 * outlive any one component that happens to be drawing it.
	 */
	const open = $derived(launcher.signInPrompt !== null || launcher.signInError !== null);

	let copied = $state(false);

	$effect(() => {
		if (launcher.signInPrompt) copied = false;
	});

	async function copy() {
		const code = launcher.signInPrompt?.user_code;
		if (!code) return;
		try {
			await navigator.clipboard.writeText(code);
			copied = true;
		} catch {
			// A webview without clipboard permission is not a failure worth a dialog: the code
			// is on screen in a font built for reading it back.
			copied = false;
		}
	}

	async function openPage() {
		const url = launcher.signInPrompt?.verification_uri;
		if (!url) return;
		try {
			const { openUrl } = await import("@tauri-apps/plugin-opener");
			await openUrl(url);
		} catch {
			window.open(url, "_blank");
		}
	}
</script>

<Dialog.Root {open} onOpenChange={(next) => !next && launcher.cancelSignIn()}>
	<Dialog.Content class="sm:max-w-[460px]">
		<Dialog.Header>
			<Dialog.Title>{t("signIn.title")}</Dialog.Title>
			<Dialog.Description>{t("signIn.description")}</Dialog.Description>
		</Dialog.Header>

		{#if launcher.signInError}
			<div
				class="flex items-start gap-2.5 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2.5"
			>
				<TriangleAlert class="mt-0.5 size-4 shrink-0 text-destructive" />
				<p class="min-w-0 text-xs leading-relaxed break-words text-destructive-foreground/90">
					{launcher.signInError}
				</p>
			</div>
		{:else if launcher.signInPrompt}
			<div class="space-y-3 py-1">
				<!-- The code is the whole dialog: big, monospaced, selectable, one click to copy. -->
				<div class="flex items-center gap-2 rounded-lg border border-border/70 bg-muted/40 px-4 py-3">
					<span
						data-selectable
						class="min-w-0 flex-1 truncate text-center font-mono text-xl tracking-[0.2em] select-text"
					>
						{launcher.signInPrompt.user_code}
					</span>
					<Button
						variant="ghost"
						size="icon"
						class="size-8 shrink-0"
						onclick={copy}
						aria-label={t("signIn.copy")}
						title={t("signIn.copy")}
					>
						{#if copied}
							<Check class="size-4 text-ok" />
						{:else}
							<Copy class="size-4" />
						{/if}
					</Button>
				</div>

				<Button variant="secondary" class="w-full" onclick={openPage}>
					<ExternalLink class="size-4" />
					{t("signIn.openPage")}
				</Button>

				<p class="truncate text-center font-mono text-xs text-muted-foreground">
					{launcher.signInPrompt.verification_uri}
				</p>

				<p class="flex items-center justify-center gap-2 text-xs text-muted-foreground">
					<Loader2 class="size-3.5 animate-spin" />
					{t("signIn.waiting")}
				</p>
			</div>
		{/if}

		<Dialog.Footer>
			<Button variant="ghost" onclick={() => launcher.cancelSignIn()}>
				{launcher.signInError ? t("common.close") : t("common.cancel")}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
