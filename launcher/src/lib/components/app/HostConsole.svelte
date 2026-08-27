<script lang="ts">
	import { ChevronDown, CornerDownLeft, Skull, X } from "@lucide/svelte";
	import { Button } from "$lib/components/ui/button";
	import { Input } from "$lib/components/ui/input";
	import { launcher } from "$lib/state.svelte";
	import { t } from "$lib/i18n.svelte";

	const server = $derived(launcher.hosts.find((entry) => entry.id === launcher.watching) ?? null);

	let box = $state<HTMLDivElement | null>(null);
	let line = $state("");
	/** Whether the view is at the bottom. Scrolling up to read has to survive new output. */
	let pinned = $state(true);

	/** Commands already sent, newest last. Up and down walk it, the way a shell does. */
	let history = $state<string[]>([]);
	let cursor = $state(-1);

	// Reading launcher.consoleLines is what subscribes this to new output.
	$effect(() => {
		launcher.consoleLines.length;
		if (pinned && box) {
			// After the DOM has the new line in it, or this scrolls to where it used to end.
			requestAnimationFrame(() => box?.scrollTo({ top: box.scrollHeight }));
		}
	});

	function onScroll() {
		if (!box) return;
		pinned = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
	}

	function send(event: SubmitEvent) {
		event.preventDefault();
		const command = line.trim();
		if (!command || !server) return;
		history = [...history.filter((entry) => entry !== command), command];
		cursor = -1;
		line = "";
		pinned = true;
		void launcher.sendCommand(server.id, command);
	}

	function walk(event: KeyboardEvent) {
		if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
		if (history.length === 0) return;
		event.preventDefault();
		const next = event.key === "ArrowUp" ? cursor + 1 : cursor - 1;
		cursor = Math.min(Math.max(next, -1), history.length - 1);
		line = cursor === -1 ? "" : history[history.length - 1 - cursor];
	}

	/**
	 * What a line is, so it can be coloured. A Minecraft server marks its own severity and
	 * says the two things worth spotting - it started, and somebody arrived - in plain words.
	 */
	function tone(text: string): string {
		if (text.startsWith(">")) return "text-primary";
		if (/\/(ERROR|FATAL)\]|Exception|\tat /.test(text)) return "text-bad";
		if (/\/WARN\]/.test(text)) return "text-warn";
		if (/joined the game|Done \(/.test(text)) return "text-ok";
		return "text-muted-foreground";
	}
</script>

{#if server}
	<!--
		A drawer under the list rather than a dialog over it: the console is watched while the
		row above it is used, and a modal would take the start button away.
	-->
	<section
		class="surface mt-2 flex min-h-0 flex-col"
		aria-label={t("host.consoleOf", { name: server.name })}
	>
		<header class="flex h-10 shrink-0 items-center gap-2 border-b border-border/50 px-4">
			<span class="size-2 shrink-0 rounded-full {server.running ? 'bg-ok' : 'bg-border'}"></span>
			<h3 class="min-w-0 truncate text-sm font-medium">{server.name}</h3>
			<span class="chip">{server.address}</span>

			<div class="flex-1"></div>

			{#if server.running}
				<!-- Only offered while it is running, and only after `stop` has been available -->
				<Button
					variant="ghost"
					size="sm"
					class="text-muted-foreground hover:text-destructive"
					onclick={() => launcher.killHost(server.id)}
					title={t("host.killHint")}
				>
					<Skull class="size-3.5" />
					{t("host.kill")}
				</Button>
			{/if}

			<Button
				variant="ghost"
				size="icon-sm"
				class="text-muted-foreground"
				onclick={() => launcher.unwatch()}
				aria-label={t("common.close")}
			>
				<X class="size-3.5" />
			</Button>
		</header>

		<div
			bind:this={box}
			onscroll={onScroll}
			data-selectable
			class="h-56 min-h-0 flex-1 overflow-y-auto bg-background/40 px-4 py-2.5 font-mono
			       text-[0.7rem] leading-[1.55]"
		>
			{#each launcher.consoleLines as text, i (i)}
				<div class="break-words whitespace-pre-wrap {tone(text)}">{text}</div>
			{:else}
				<p class="text-muted-foreground/60">
					{server.running ? t("host.consoleQuiet") : t("host.consoleStopped")}
				</p>
			{/each}
		</div>

		<!-- Scrolled up: say so, and offer the way back rather than yanking the view down. -->
		{#if !pinned}
			<button
				type="button"
				onclick={() => {
					pinned = true;
					box?.scrollTo({ top: box.scrollHeight });
				}}
				class="flex items-center justify-center gap-1.5 border-t border-border/50 py-1
				       text-xs text-muted-foreground transition-colors hover:text-foreground"
			>
				<ChevronDown class="size-3" />
				{t("host.consoleFollow")}
			</button>
		{/if}

		<form onsubmit={send} class="flex shrink-0 items-center gap-2 border-t border-border/50 p-2">
			<Input
				bind:value={line}
				onkeydown={walk}
				disabled={!server.running}
				placeholder={server.running ? t("host.commandHint") : t("host.consoleStopped")}
				class="h-8 flex-1 border-0 bg-transparent font-mono text-xs shadow-none focus-visible:ring-0"
				aria-label={t("host.command")}
			/>
			<Button
				type="submit"
				variant="ghost"
				size="icon-sm"
				disabled={!server.running || !line.trim()}
				aria-label={t("host.send")}
			>
				<CornerDownLeft class="size-3.5" />
			</Button>
		</form>
	</section>
{/if}
