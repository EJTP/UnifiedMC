<script lang="ts" module>
	export type ServerState = "checking" | "offline" | "ready" | "unknown";
</script>

<script lang="ts">
	import { cn } from "$lib/utils";

	let { state, class: className = "" }: { state: ServerState; class?: string } = $props();

	const tone: Record<ServerState, string> = {
		ready: "bg-ok",
		unknown: "bg-warn",
		offline: "bg-bad",
		checking: "bg-muted-foreground"
	};

	const label: Record<ServerState, string> = {
		ready: "erreichbar",
		unknown: "erreichbar, ohne Manifest",
		offline: "nicht erreichbar",
		checking: "wird geprüft"
	};
</script>

<span class={cn("relative flex size-3 shrink-0", className)} title={label[state]}>
	{#if state === "checking"}
		<span
			class="absolute inline-flex size-full animate-ping rounded-full bg-muted-foreground opacity-60"
		></span>
	{/if}
	<span class={cn("relative inline-flex size-3 rounded-full", tone[state])}></span>
	<span class="sr-only">{label[state]}</span>
</span>
