import { call, onProgress } from "./bridge";
import type { Hit, Progress, SavedServer, ServerStatus, Session, Settings } from "./types";

export type Tab = "search" | "installed" | "pack";

/** Must match PAGE in the Rust side, or "more" skips or repeats results. */
const PAGE = 40;

/**
 * Everything the window shows. The backend pushes into this; no screen fetches on its own,
 * so a screen can never be half-populated by its own timing.
 */
class LauncherState {
	servers = $state<SavedServer[]>([]);
	status = $state<Record<string, ServerStatus>>({});
	settings = $state<Settings>({ memory: 0, offline_name: "Player", manifest_port: 25566, keep_open: true });
	session = $state<Session | null>(null);
	progress = $state<Progress | null>(null);
	playing = $state<string | null>(null);
	error = $state<string | null>(null);

	async start() {
		const boot = await call<{ servers: SavedServer[]; settings: Settings; session: Session }>(
			"bootstrap"
		);
		this.servers = boot.servers;
		this.settings = boot.settings;
		this.session = boot.session;

		await onProgress((progress) => (this.progress = progress));
		this.probeAll();
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
				motd: "",
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
			this.error = String(error);
		}
	}

	async remove(id: string) {
		this.servers = await call<SavedServer[]>("remove_server", { id });
		delete this.status[id];
	}

	async play(server: SavedServer) {
		if (this.playing) return;
		this.playing = server.id;
		this.error = null;
		this.progress = { phase: "Wird vorbereitet", detail: server.address, done: 0, total: 0 };
		try {
			await call("play", { address: server.address });
		} catch (error) {
			this.error = String(error);
		} finally {
			this.playing = null;
			this.progress = null;
		}
	}

	/** The mod browser, for one server at a time. */
	browsing = $state<SavedServer | null>(null);
	tab = $state<Tab>("search");
	hits = $state<Hit[]>([]);
	picked = $state<Set<string>>(new Set());
	loadingMods = $state(false);
	note = $state("");

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
				this.note = this.tab === "installed" ? "Noch nichts eigenes" : "Nichts gefunden";
			}
		} catch (error) {
			if (mine !== this.#request) return;
			if (!append) this.hits = [];
			this.note = String(error);
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
				this.note = `${gone.length} entfernt`;
			} else {
				const added = await call<string[]>("install_mods", {
					address: this.browsing.address,
					ids
				});
				this.note = `${added.length} installiert, liegt in deinem Profil`;
			}
			this.picked = new Set();
			await this.loadMods("");
		} catch (error) {
			this.note = String(error);
		} finally {
			this.loadingMods = false;
		}
	}

	async saveSettings(next: Settings) {
		this.settings = await call<Settings>("save_settings", { settings: next });
	}
}

export const launcher = new LauncherState();
