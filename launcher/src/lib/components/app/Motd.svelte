<script lang="ts">
	import type { MotdSpan } from "$lib/types";

	let { spans }: { spans: MotdSpan[] } = $props();

	/** The unstyled text, for the tooltip - a truncated description has to be readable somehow. */
	const plain = $derived(spans.map((span) => span.text).join(""));
</script>

<!--
	The description as the server styled it. Minecraft draws §k as characters that keep
	changing; here it is just blurred, which reads as "hidden" without a timer running for
	something nobody looks at twice.

	block, not inline: an inline span clips at its parent's edge but never draws the ellipsis,
	and a server description is long often enough that the cut has to be visible.
-->
<span class="block min-w-0 truncate" title={plain}>
	{#each spans as span, i (i)}<span
			style:color={span.color}
			style:font-weight={span.bold ? "600" : undefined}
			style:font-style={span.italic ? "italic" : undefined}
			style:text-decoration={[
				span.underlined ? "underline" : "",
				span.strikethrough ? "line-through" : ""
			]
				.filter(Boolean)
				.join(" ") || undefined}
			class={span.obfuscated ? "blur-[2px]" : ""}>{span.text}</span
		>{/each}
</span>
