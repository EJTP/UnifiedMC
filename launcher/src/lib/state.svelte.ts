import { call, onProgress } from "./bridge";
import { locale, resolveLocale, t, translate } from "./i18n.svelte";
import type {
	Hit,
	Instance,
	Progress,
	SavedServer,
	ServerStatus,
	Session,
	Settings
} from "./types";

/** Which of the two halves the window is showing. */
export type View = "servers" | "instances";

export type Tab = "search" | "installed" | "pack";

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
		this.picked = new Set();
		this.note = "";
		void this.loadMods("");
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
			if (this.tab === "installed") {
				const gone = await call<string[]>("remove_mods", {
					address: this.browsing.address,
					names: ids
				});
				this.note =
					gone.length > 0 ? t("mods.removedCount", { count: gone.length }) : t("mods.removedNone");
			} else {
				const added = await call<string[]>("install_mods", {
					address: this.browsing.address,
					ids
				});
				// zero is not success: the catalogue had nothing for this version and said so by
				// handing back an empty list
				this.note =
					added.length > 0
						? t("mods.installedCount", { count: added.length })
						: t("mods.installedNone");
			}
			this.picked = new Set();
			await this.loadMods("");
		} catch (error) {
			this.note = translate(String(error));
		} finally {
			this.loadingMods = false;
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
