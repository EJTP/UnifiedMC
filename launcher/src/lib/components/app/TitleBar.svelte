<script lang="ts">
	import { Minus, Square, X } from "@lucide/svelte";
	import { inTauri } from "$lib/bridge";
	import { t } from "$lib/i18n.svelte";

	/**
	 * The window's own bar, because the system one is off.
	 *
	 * Everything here is loaded on demand: in a plain browser there is no window to minimise,
	 * and importing the API at the top would fail the module before the page draws at all.
	 */
	async function window_() {
		const { getCurrentWindow } = await import("@tauri-apps/api/window");
		return getCurrentWindow();
	}

	const button =
		"flex h-9 w-11 shrink-0 items-center justify-center text-muted-foreground outline-none" +
		" transition-colors hover:bg-accent/60 hover:text-foreground focus-visible:ring-3" +
		" focus-visible:ring-inset focus-visible:ring-ring/50";
</script>

<!--
	data-tauri-drag-region on the strip, not on what sits in it: a button inside a drag region
	moves the window instead of being pressed.
-->
<header
	data-tauri-drag-region
	class="flex h-9 shrink-0 items-center border-b border-border/60 bg-card/60 pl-3"
>
	<img src="/mark.png" alt="" class="pointer-events-none size-4" />
	<span class="pointer-events-none ml-2 truncate text-xs font-medium text-muted-foreground">
		UnifiedMC
	</span>

	<div class="flex-1"></div>

	{#if inTauri}
		<button
			type="button"
			class={button}
			aria-label={t("window.minimise")}
			title={t("window.minimise")}
			onclick={async () => (await window_()).minimize()}
		>
			<Minus class="size-4" />
		</button>
		<button
			type="button"
			class={button}
			aria-label={t("window.maximise")}
			title={t("window.maximise")}
			onclick={async () => (await window_()).toggleMaximize()}
		>
			<Square class="size-3.5" />
		</button>
		<!-- The one destructive control in the bar is the only one that turns red. -->
		<button
			type="button"
			class="{button} hover:bg-destructive hover:text-destructive-foreground"
			aria-label={t("common.close")}
			title={t("common.close")}
			onclick={async () => (await window_()).close()}
		>
			<X class="size-4" />
		</button>
	{/if}
</header>
