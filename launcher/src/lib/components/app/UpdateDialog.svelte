<script lang="ts">
	import { ArrowUpCircle, Check, ExternalLink, Loader2, RotateCw, TriangleAlert } from "@lucide/svelte";
	import * as Dialog from "$lib/components/ui/dialog";
	import { Button } from "$lib/components/ui/button";
	import { Progress } from "$lib/components/ui/progress";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";

	let { open = $bindable(false) }: { open: boolean } = $props();

	const release = $derived(launcher.release);

	/** Null while the download has no total to measure against, which is a sweep not a bar. */
	const fraction = $derived(
		launcher.updateTotal && launcher.updateTotal > 0
			? (launcher.updateBytes / launcher.updateTotal) * 100
			: null
	);

	function megabytes(bytes: number) {
		return (bytes / 1_048_576).toFixed(1);
	}

	/**
	 * The release notes as plain lines. Markdown from the release body, shown as text: a
	 * dialog that renders somebody's markdown is a dialog that renders whatever is in it.
	 */
	const notes = $derived(
		(release?.notes ?? "")
			.split("\n")
			.map((line) => line.replace(/^#+\s*/, "").trim())
			.filter(Boolean)
			.slice(0, 14)
	);
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-[520px]">
		<Dialog.Header>
			<Dialog.Title class="flex items-center gap-2">
				<ArrowUpCircle class="size-4 text-primary" />
				{t("update.title", { version: release?.latest ?? "" })}
			</Dialog.Title>
			<Dialog.Description>
				{t("update.from", { current: release?.current ?? "" })}
			</Dialog.Description>
		</Dialog.Header>

		<div class="max-h-[46vh] space-y-3 overflow-y-auto py-2 pr-1">
			{#if notes.length > 0}
				<div class="space-y-1 rounded-lg bg-muted/40 px-3.5 py-3" data-selectable>
					{#each notes as line, i (i)}
						<p class="text-xs leading-relaxed text-muted-foreground">{line}</p>
					{/each}
				</div>
			{/if}

			{#if launcher.updateError}
				<div
					class="flex items-start gap-2.5 rounded-lg border border-destructive/40
					       bg-destructive/10 px-3.5 py-2.5"
				>
					<TriangleAlert class="mt-0.5 size-4 shrink-0 text-destructive" />
					<p class="min-w-0 text-xs leading-relaxed break-words text-destructive-foreground/90">
						{launcher.updateError}
					</p>
				</div>
			{/if}

			{#if launcher.updating}
				<div class="space-y-2">
					{#if fraction === null}
						<!-- No content length to measure against: a sweep says "working", a bar at
						     zero says "stuck". -->
						<div class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
							<div
								class="h-full w-1/3 animate-[sweep_1.4s_ease-in-out_infinite] rounded-full bg-primary motion-reduce:w-full"
							></div>
						</div>
					{:else}
						<Progress value={fraction} class="h-1.5" />
					{/if}
					<p class="text-xs text-muted-foreground">
						{launcher.updateTotal
							? t("update.downloading", {
									done: megabytes(launcher.updateBytes),
									total: megabytes(launcher.updateTotal)
								})
							: t("update.downloadingUnknown", { done: megabytes(launcher.updateBytes) })}
					</p>
				</div>
			{/if}

			{#if launcher.updateDone}
				<div class="flex items-start gap-2.5 rounded-lg border border-ok/40 bg-ok/5 px-3.5 py-2.5">
					<Check class="mt-0.5 size-4 shrink-0 text-ok" />
					<p class="min-w-0 text-xs leading-relaxed">{t("update.installed")}</p>
				</div>
			{/if}
		</div>

		<Dialog.Footer>
			<!-- The page, for anyone who would rather read the whole thing than press a button. -->
			<Button
				variant="ghost"
				size="sm"
				class="mr-auto text-muted-foreground"
				onclick={() => launcher.openRelease()}
			>
				<ExternalLink class="size-3.5" />
				{t("update.openPage")}
			</Button>

			{#if launcher.updateDone}
				<Button class="cta-glow bg-cta text-cta-foreground hover:bg-cta/90" onclick={() => launcher.restart()}>
					<RotateCw class="size-4" />
					{t("update.restart")}
				</Button>
			{:else}
				<Button variant="ghost" disabled={launcher.updating} onclick={() => (open = false)}>
					{t("update.later")}
				</Button>
				<Button disabled={launcher.updating} onclick={() => launcher.installUpdate()}>
					{#if launcher.updating}
						<Loader2 class="size-4 animate-spin" />
						{t("update.installing")}
					{:else}
						<ArrowUpCircle class="size-4" />
						{t("update.install")}
					{/if}
				</Button>
			{/if}
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
