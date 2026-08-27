<script lang="ts">
	import { ArrowUpCircle, Minus, Square, X } from "@lucide/svelte";
	import { inTauri } from "$lib/bridge";
	import { t } from "$lib/i18n.svelte";
	import { launcher } from "$lib/state.svelte";

	/**
	 * The window's own bar, because the system one is off. The API is loaded on demand: in a
	 * plain browser there is no window to minimise.
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

	<!--
		A newer release, said once and quietly. In the bar rather than over the list: an update
		is never urgent enough to interrupt what somebody opened the launcher to do, and the
		one place always on screen is the one place it cannot be missed either.
	-->
	{#if launcher.release && !launcher.updateDismissed}
		<button
			type="button"
			onclick={() => launcher.openRelease()}
			title={t("update.hint", { version: launcher.release.latest })}
			class="mr-1 flex items-center gap-1.5 rounded-full bg-primary/15 px-2.5 py-1 text-xs
			       text-foreground/90 outline-none transition-colors hover:bg-primary/25
			       focus-visible:ring-3 focus-visible:ring-ring/50"
		>
			<ArrowUpCircle class="size-3.5 text-primary" />
			{t("update.available", { version: launcher.release.latest })}
		</button>
		<button
			type="button"
			onclick={() => (launcher.updateDismissed = true)}
			aria-label={t("update.dismiss")}
			title={t("update.dismiss")}
			class="mr-2 rounded text-muted-foreground/60 outline-none transition-colors
			       hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
		>
			<X class="size-3" />
		</button>
	{/if}

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
