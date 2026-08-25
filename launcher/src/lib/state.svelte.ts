import { call, onProgress, onSignIn } from "./bridge";
import { locale, resolveLocale, t, translate } from "./i18n.svelte";
import type {
	Hit,
	Instance,
	Kind,
	Progress,
	SavedServer,
	ServerStatus,
	Session,
	Settings,
	SignInPrompt
} from "./types";

/** Which of the two halves the window is showing. */
export type View = "servers" | "instances";

export type Tab = "search" | "installed" | "pack";

/** What a row in the browser is: the four things a manifest and a catalogue both know about. */
export type { Kind };

/** In the order they are offered. The backend answers allowed_kinds with a subset of these. */
export const KINDS: Kind[] = ["mod", "resourcepack", "shader", "datapack"];

/** Must match PAGE in the Rust side, or "more" skips or repeats results. */
const PAGE = 40;

/**
 * Everything the window shows. The backend pushes into this; no screen fetches on its own,
 * so a screen can never be half-populated by its own timing.
 */
class LauncherState {
	view = $state<View>("servers");
	servers = $state<SavedServer[]>([]);
	instances = $state<Instance[]>([]);
	status = $state<Record<string, ServerStatus>>({});
	settings = $state<Settings>({
		language: "system",
		memory: 0,
		offline_name: "Player",
		manifest_port: 25566,
		keep_open: true,
		curseforge_key: "",
		jvm_profile: "balanced",
		jvm_args: ""
	});
	session = $state<Session | null>(null);
	progress = $state<Progress | null>(null);
	/** Minecraft's own "no icon" texture, read from the copy on this machine. */
	unknownServerIcon = $state<string | null>(null);
	/** The player's face, from their skin. Fetched separately so the window draws first. */
	playerHead = $state<string | null>(null);
	playing = $state<string | null>(null);
	error = $state<string | null>(null);
	/** Whether the stored lists have arrived. Before that, empty means "not read yet". */
	booted = $state(false);

	async start() {
		let boot;
		try {
			boot = await call<{
				servers: SavedServer[];
				settings: Settings;
				session: Session;
				unknown_server_icon: string | null;
			}>("bootstrap");
		} catch (error) {
			// Nothing else can run, so the window has to say why rather than sit on "loading".
			this.error = translate(String(error));
			this.booted = true;
			return;
		}
		this.servers = boot.servers;
		this.settings = boot.settings;
		this.applyLanguage();
		this.session = boot.session;
		this.unknownServerIcon = boot.unknown_server_icon;

		// Rust names its phases as dotted keys; anything a library threw stays as it came.
		await onProgress((progress) => {
			this.progress = {
				...progress,
				phase: translate(progress.phase),
				detail: translate(progress.detail)
			};
		});
		this.probeAll();

		// both need the network, so neither may hold up the first paint
		void call<string | null>("player_head").then((head) => (this.playerHead = head));
		void this.loadVersions();
		void call<Instance[]>("instances")
			.then((list) => (this.instances = list))
			// however it went, the lists are as full as they are going to get - an empty screen
			// may say so now
			.finally(() => (this.booted = true));
	}

	/**
	 * Switch halves.
	 *
	 * The mod browser draws over whichever list is behind it, so leaving it open across a
	 * switch means pressing "Servers" and still looking at an instance's mods - the nav says
	 * one thing and the screen shows another.
	 */
	show(view: View) {
		this.view = view;
		if (this.browsing) this.closeMods();
	}

	/** Ask about every server at once. A slow one must not hold up the rest of the list. */
	probeAll() {
		for (const server of this.servers) {
			void this.probe(server);
		}
	}

	async probe(server: SavedServer) {
		try {
			const status = await call<ServerStatus>("probe", { id: server.id, address: server.address });
			this.status[server.id] = status;
		} catch (error) {
			this.status[server.id] = {
				id: server.id,
				online: false,
				error: String(error),
				motd: [],
				icon: null,
				players: 0,
				max_players: 0,
				manifest: null
			};
		}
	}

	async add(name: string, address: string) {
		this.error = null;
		try {
			this.servers = await call<SavedServer[]>("add_server", { name, address });
			this.probeAll();
		} catch (error) {
			this.error = translate(String(error));
		}
	}

	/**
	 * Choose a loader for a server that does not run one.
	 *
	 * A Vanilla or Paper server announces nothing, so without a choice the instance is plain
	 * Minecraft and loads no mods. Client-side mods do not need the server to know about them.
	 */
	async configure(server: SavedServer, minecraft: string | null, loader: string | null) {
		this.error = null;
		try {
			this.servers = await call<SavedServer[]>("configure", {
				id: server.id,
				minecraft,
				loader
			});
			await this.probe(this.servers.find((s) => s.id === server.id) ?? server);
		} catch (error) {
			this.error = translate(String(error));
		}
	}

	/** Every Minecraft release, for the version picker. Fetched once. */
	versions = $state<string[]>([]);

	async loadVersions() {
		if (this.versions.length) return;
		try {
			this.versions = await call<string[]>("versions");
		} catch {
			this.versions = [];
		}
	}

	/** The id of what was just created, so a caller can select it - or null if it failed. */
	async addInstance(
		name: string,
		minecraft: string,
		loader: string | null,
		loaderVersion: string | null = null
	): Promise<string | null> {
		this.error = null;
		try {
			// The command answers with the whole list and appends, so the new one is the last.
			const list = await call<Instance[]>("add_instance", {
				name,
				minecraft,
				loader,
				loaderVersion
			});
			this.instances = list;
			return list.at(-1)?.id ?? null;
		} catch (error) {
			this.error = translate(String(error));
			return null;
		}
	}

	async removeInstance(id: string) {
		this.instances = await call<Instance[]>("remove_instance", { id });
	}

	async playInstance(instance: Instance) {
		if (this.playing) return;
		this.playing = instance.id;
		this.error = null;
		this.progress = { phase: t("progress.prepare"), detail: instance.name, done: 0, total: 0 };
		try {
			await call("play_instance", { id: instance.id });
		} catch (error) {
			this.error = translate(String(error));
		} finally {
			this.playing = null;
			this.progress = null;
		}
	}

	/** The server whose profile question is open, if one is. */
	choosing = $state<SavedServer | null>(null);

	/**
	 * Ask before starting, unless the server has already answered.
	 *
	 * A server that publishes a pack has decided what runs, and a dialog there would only ask
	 * the player to confirm the one possible answer. A server that publishes none - vanilla,
	 * Paper, anything without the mod - decides nothing, so the question has to be asked even
	 * when no instance fits yet: creating one is reachable from that dialog and nowhere else.
	 */
	askThenPlay(server: SavedServer) {
		if (this.playing) return;
		const manifest = this.status[server.id]?.manifest;
		if (manifest && manifest.mods.length > 0) {
			void this.play(server);
			return;
		}
		this.choosing = server;
	}

	async play(server: SavedServer, instance: string | null = null) {
		if (this.playing) return;
		this.choosing = null;
		this.playing = server.id;
		this.error = null;
		this.progress = { phase: t("progress.prepare"), detail: server.name, done: 0, total: 0 };
		try {
			await call("play", { address: server.address, instance });
		} catch (error) {
			this.error = translate(String(error));
		} finally {
			this.playing = null;
			this.progress = null;
		}
	}

	async remove(id: string) {
		this.servers = await call<SavedServer[]>("remove_server", { id });
		delete this.status[id];
	}

	/** The mod browser, for one server at a time. */
	browsing = $state<SavedServer | null>(null);
	tab = $state<Tab>("search");
	kind = $state<Kind>("mod");
	/**
	 * What the server lets a player add on top of what it ships. All four until it says
	 * otherwise, so a server that never answers is browsable rather than empty - the install
	 * itself is refused in Rust either way, and that refusal is the one that counts.
	 */
	allowedKinds = $state<Kind[]>([...KINDS]);
	hits = $state<Hit[]>([]);
	picked = $state<Set<string>>(new Set());
	loadingMods = $state(false);
	note = $state("");

	/** The mod browser works against a server or an instance; both name an address to ask. */
	openInstanceMods(instance: Instance) {
		this.openMods({
			id: instance.id,
			name: instance.name,
			address: `instance-${instance.id}`,
			loader: instance.loader,
			minecraft: instance.minecraft
		});
	}

	openMods(server: SavedServer) {
		this.browsing = server;
		this.tab = "search";
		this.kind = "mod";
		this.allowedKinds = [...KINDS];
		this.picked = new Set();
		this.note = "";
		void this.loadAllowedKinds(server.address);
		void this.loadMods("");
	}

	async loadAllowedKinds(address: string) {
		try {
			const allowed = await call<string[]>("allowed_kinds", { address });
			// The browser may have moved on to another server while this was in flight.
			if (this.browsing?.address !== address) return;
			this.allowedKinds = KINDS.filter((kind) => allowed.includes(kind));
			if (!this.allowedKinds.includes(this.kind)) {
				await this.switchKind(this.allowedKinds[0] ?? "mod");
			}
		} catch {
			// An unreachable server tells us nothing about its rules; leave all four offered.
		}
	}

	closeMods() {
		this.browsing = null;
		this.hits = [];
	}

	async switchTab(tab: Tab) {
		this.tab = tab;
		this.picked = new Set();
		await this.loadMods("");
	}

	/** A selection is a list of ids in one category; carrying it across would install nonsense. */
	async switchKind(kind: Kind) {
		this.kind = kind;
		this.picked = new Set();
		await this.loadMods("");
	}

	/** Only the newest request may write the list; a slow earlier one must not overwrite it. */
	#request = 0;

	query = $state("");
	more = $state(false);
	#offset = 0;

	async loadMods(query: string, append = false) {
		if (!this.browsing) return;
		const mine = ++this.#request;
		this.query = query;
		this.#offset = append ? this.#offset : 0;
		this.loadingMods = true;
		if (!append) this.note = "";

		try {
			const hits = await call<Hit[]>("mods", {
				address: this.browsing.address,
				tab: this.tab,
				kind: this.kind,
				query,
				offset: this.#offset
			});
			if (mine !== this.#request) return;

			// The side filter drops a good share of every page, so an empty page does not mean
			// the catalogue is exhausted - only a short one does.
			this.more = this.tab === "search" && hits.length > 0;
			this.#offset += PAGE;
			this.hits = append ? [...this.hits, ...hits] : hits;

			if (this.hits.length === 0) {
				this.note = t(this.tab === "installed" ? "mods.nothingOwn" : "mods.notFound");
			}
		} catch (error) {
			if (mine !== this.#request) return;
			if (!append) this.hits = [];
			this.note = translate(String(error));
		} finally {
			if (mine === this.#request) this.loadingMods = false;
		}
	}

	loadMore() {
		return this.loadMods(this.query, true);
	}

	toggle(id: string) {
		const next = new Set(this.picked);
		next.has(id) ? next.delete(id) : next.add(id);
		this.picked = next;
	}

	async applyMods() {
		if (!this.browsing || this.picked.size === 0) return;
		const ids = [...this.picked];
		this.loadingMods = true;
		try {
			let said: string;
			if (this.tab === "installed") {
				const gone = await call<string[]>("remove_mods", {
					address: this.browsing.address,
					kind: this.kind,
					names: ids
				});
				said =
					gone.length > 0 ? t("mods.removedCount", { count: gone.length }) : t("mods.removedNone");
			} else {
				const added = await call<string[]>("install_mods", {
					address: this.browsing.address,
					kind: this.kind,
					ids
				});
				// zero is not success: the catalogue had nothing for this version and said so by
				// handing back an empty list
				said =
					added.length > 0
						? t("mods.installedCount", { count: added.length })
						: t("mods.installedNone");
			}
			this.picked = new Set();
			// After the reload, not before it: loadMods clears the note on its way in, so a
			// message set first is wiped before the player ever reads it.
			await this.loadMods("");
			this.note = said;
		} catch (error) {
			this.note = translate(String(error));
		} finally {
			this.loadingMods = false;
		}
	}

	/**
	 * Signing in, and what the player has to do while it waits.
	 *
	 * The prompt arrives as an event rather than as the call's return value, because the call
	 * does not return until they have finished in their browser - which is the whole point of
	 * the device flow, and would otherwise be several minutes of nothing on screen.
	 */
	signInPrompt = $state<SignInPrompt | null>(null);
	signingIn = $state(false);
	signInError = $state<string | null>(null);

	async signIn() {
		if (this.signingIn) return;
		this.signingIn = true;
		this.signInError = null;
		const stop = await onSignIn((prompt) => (this.signInPrompt = prompt));
		try {
			this.session = await call<Session>("sign_in");
			this.signInPrompt = null;
			await this.refreshHead();
		} catch (error) {
			this.signInError = translate(String(error));
		} finally {
			stop();
			this.signingIn = false;
		}
	}

	/** Cancelling only stops us waiting; the code stays valid until Microsoft expires it. */
	cancelSignIn() {
		this.signInPrompt = null;
		this.signInError = null;
	}

	async signOut() {
		this.session = await call<Session>("sign_out");
		this.playerHead = null;
		await this.refreshHead();
	}

	/**
	 * The skin, from the webview's own file picker.
	 *
	 * Both answer with null when it worked and with the reason when it did not, so the dialog
	 * can show it where the player is looking instead of in the window-wide error bar.
	 */
	async setSkin(pngBase64: string, slim: boolean): Promise<string | null> {
		try {
			await call("set_skin", { pngBase64, slim });
			await this.refreshHead();
			return null;
		} catch (error) {
			return translate(String(error));
		}
	}

	async resetSkin(): Promise<string | null> {
		try {
			await call("reset_skin");
			await this.refreshHead();
			return null;
		} catch (error) {
			return translate(String(error));
		}
	}

	/** The face in the sidebar is the only proof the change went through. */
	async refreshHead() {
		try {
			this.playerHead = await call<string | null>("player_head");
		} catch {
			// a stale face is better than none, and the change itself already reported
		}
	}

	async saveSettings(next: Settings) {
		this.settings = await call<Settings>("save_settings", { settings: next });
		this.applyLanguage();
	}

	/** Put the chosen language into effect. Every screen reads it through t(). */
	applyLanguage() {
		locale.current = resolveLocale(this.settings.language ?? "system");
	}
}

export const launcher = new LauncherState();
