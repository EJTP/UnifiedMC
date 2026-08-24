<script lang="ts">
	import type { ServerState } from "$lib/types";
	import { cn } from "$lib/utils";

	let { state, class: className = "" }: { state: ServerState; class?: string } = $props();

	const tone: Record<ServerState, string> = {
		ready: "bg-ok",
		"needs-swap": "bg-warn",
		offline: "bg-bad",
		checking: "bg-muted-foreground",
		unknown: "bg-muted-foreground"
	};

	const label: Record<ServerState, string> = {
		ready: "bereit",
		"needs-swap": "Wechsel nötig",
		offline: "nicht erreichbar",
		checking: "wird geprüft",
		unknown: "unbekannt"
	};
</script>

<span class={cn("relative flex size-2.5 shrink-0", className)} title={label[state]}>
	{#if state === "checking"}
		<span class="absolute inline-flex size-full animate-ping rounded-full bg-muted-foreground opacity-60"
		></span>
	{/if}
	<span class={cn("relative inline-flex size-2.5 rounded-full", tone[state])}></span>
	<span class="sr-only">{label[state]}</span>
</span>
