<script lang="ts">
	import { Button } from "$lib/components/ui/button";
	import { Progress } from "$lib/components/ui/progress";
	import { t, translate } from "$lib/i18n.svelte";
	import type { Progress as Job } from "$lib/types";

	// No handler means there is nothing to stop - the shutdown overlay waits on worlds saving.
	let { job, oncancel }: { job: Job; oncancel?: () => void } = $props();

	// Local, not on the launcher: the overlay lives exactly as long as the job does, so this
	// resets itself and no field has to be cleared anywhere.
	let asked = $state(false);

	const fraction = $derived(job.total > 0 ? (job.done / job.total) * 100 : null);

	// Rust names its phases as dotted keys; anything a library threw arrives as English prose.
	const phase = $derived(translate(job.phase));
	const detail = $derived(translate(job.detail));
</script>

<div
	class="absolute inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm"
>
	<div class="w-[340px] max-w-[calc(100%-2rem)] rounded-xl border border-border bg-card p-6 shadow-2xl">
		<p class="truncate text-center text-sm font-medium" title={phase}>{phase}</p>

		<div class="mt-4">
			{#if fraction === null}
				<!--
					No count to show: a bar that sweeps says "working", a bar at zero says "stuck".
					motion-reduce fills the track instead - the animation is disabled globally there,
					and a frozen sweep would leave an empty bar behind.
				-->
				<div class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
					<div
						class="h-full w-1/3 animate-[sweep_1.4s_ease-in-out_infinite] rounded-full bg-primary motion-reduce:w-full"
					></div>
				</div>
			{:else}
				<Progress value={fraction} class="h-1.5" />
			{/if}
		</div>

		<div class="mt-3 flex items-baseline justify-between gap-3 text-xs text-muted-foreground">
			<span class="min-w-0 flex-1 truncate font-mono" title={detail}>{detail}</span>
			{#if job.total > 0}
				<span class="shrink-0 font-mono">{job.done} / {job.total}</span>
			{/if}
		</div>

		{#if oncancel}
			<!--
				The label changes rather than the button going away: the stop lands at the next file,
				and a button that vanished would read as "already stopped" while bytes still arrive.
			-->
			<div class="mt-4 flex justify-center">
				<Button
					variant="ghost"
					size="sm"
					disabled={asked}
					onclick={() => {
						asked = true;
						oncancel?.();
					}}
				>
					{asked ? t("progress.cancelling") : t("common.cancel")}
				</Button>
			</div>
		{/if}
	</div>
</div>
