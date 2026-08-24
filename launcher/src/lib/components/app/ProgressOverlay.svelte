<script lang="ts">
	import { Progress } from "$lib/components/ui/progress";
	import type { Progress as Job } from "$lib/types";

	let { job }: { job: Job } = $props();

	const fraction = $derived(job.total > 0 ? (job.done / job.total) * 100 : null);
</script>

<div
	class="absolute inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm"
>
	<div class="w-[340px] rounded-xl border border-border bg-card p-6 shadow-2xl">
		<p class="text-center text-sm font-medium">{job.phase}</p>

		<div class="mt-4">
			{#if fraction === null}
				<!-- No count to show: a bar that sweeps says "working", a bar at zero says "stuck". -->
				<div class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
					<div class="h-full w-1/3 animate-[sweep_1.4s_ease-in-out_infinite] rounded-full bg-primary"></div>
				</div>
			{:else}
				<Progress value={fraction} class="h-1.5" />
			{/if}
		</div>

		<div class="mt-3 flex items-baseline justify-between gap-3 text-xs text-muted-foreground">
			<span class="truncate font-mono">{job.detail}</span>
			{#if job.total > 0}
				<span class="shrink-0 font-mono">{job.done} / {job.total}</span>
			{/if}
		</div>
	</div>
</div>

<style>
	@keyframes sweep {
		0% { transform: translateX(-100%); }
		100% { transform: translateX(300%); }
	}
</style>
