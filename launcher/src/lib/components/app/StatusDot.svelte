<script lang="ts" module>
	export type ServerState = "checking" | "offline" | "ready" | "unknown";
</script>

<script lang="ts">
	import { CircleCheck, CircleQuestionMark, CircleX, Loader2 } from "@lucide/svelte";
	import { t } from "$lib/i18n.svelte";
	import { cn } from "$lib/utils";

	let { state, class: className = "" }: { state: ServerState; class?: string } = $props();

	// A glyph and not only a colour: green against red is the one distinction a fair share of
	// players cannot make, and the card's left stripe is colour and nothing else.
	const mark = {
		ready: { icon: CircleCheck, tone: "text-ok" },
		unknown: { icon: CircleQuestionMark, tone: "text-warn" },
		offline: { icon: CircleX, tone: "text-bad" },
		checking: { icon: Loader2, tone: "text-muted-foreground" }
	};

	const Mark = $derived(mark[state].icon);

	// Derived, not a lookup table built once: the label has to follow a language change.
	const label = $derived(t(`status.${state}`));
</script>

<!-- flex, not inline: an svg in an inline box hangs off the text baseline and rides a pixel high -->
<span class={cn("flex shrink-0", className)} title={label}>
	<Mark class={cn("size-4", mark[state].tone, state === "checking" && "animate-spin")} />
	<span class="sr-only">{label}</span>
</span>
