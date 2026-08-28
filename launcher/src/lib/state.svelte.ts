import {
	call,
	downloadAndInstall,
	onClosing,
	onConsole,
	onHosts,
	onProgress,
	onRunning,
	onSignIn,
	pickPack,
	relaunch
} from "./bridge";
import { locale, resolveLocale, t, translate } from "./i18n.svelte";
import { applyTheme } from "./theme";
import type {
	ConsoleLine,
	Hit,
	HostedServer,
	Instance,
	Kind,
	Played,
	Progress,
	Release,
	SavedServer,
	ServerStatus,
	Session,
	Settings,
	SignInPrompt
} from "./types";

/** Which list the window is showing. */
export type View = "servers" | "instances" | "hosting";

/**
 * Two, not three. "What the server ships" and "what I added" were separate tabs answering one
 * question - what is actually here - and the difference that matters, whether a thing can be
 * taken away again, belongs to the row rather than to the tab it was found on.
 */
export type Tab = "search" | "installed";

/** How the catalogue is ordered. Must match the Sort enum on the Rust side. */
export type Sort = "relevance" | "downloads" | "follows" | "updated" | "newest";

export const SORTS: Sort[] = ["relevance", "downloads", "follows", "updated", "newest"];

/** What a row in the browser is: the four things a manifest and a catalogue both know about. */
export type { Kind };

/** In the order they are offered. The backend answers allowed_kinds with a subset of these. */
export const KINDS: Kind[] = ["mod", "resourcepack", "shader", "datapack"];

/** Must match PAGE in the Rust side, or "more" skips or repeats results. */
const PAGE = 40;

/** How much console to keep in the window. Rust keeps its own, larger, buffer. */
const SCROLLBACK = 600;

/**
 * An error from the backend, plus what to do about it when we can tell.
 *
 * Windows refuses to replace a file another process holds open and reports it as "Access is
 * denied (os error 5)" - and the process is almost always a Minecraft that is still running.
 */
function describe(error: unknown): string {
	const raw = translate(String(error));
	const denied = /os error 5|access is denied|zugriff verweigert/i.test(raw);
	return denied ? `${raw}\n\n${t("error.accessDeniedHint")}` : raw;
}

/** Everything the window shows. The backend pushes into this; no screen fetches on its own. */
class LauncherState {
	view = $state<View>("servers");
	servers = $state<SavedServer[]>([]);
	instances = $state<Instance[]>([]);
	status = $state<Record<string, ServerStatus>>({});
	settings = $state<Settings>({
		language: "system",
		memory: 0,
		offline_name: "Player",
		keep_open: true,
		jvm_profile: "balanced",
		jvm_args: "",
		accent: "violet",
		accent_primary: "#7c3aed",
		accent_cta: "#f43f5e",
		backdrop: "midnight"
	});
	session = $state<Session | null>(null);
	progress = $state<Progress | null>(null);
	/** Minecraft's own "no icon" texture, read from the copy on this machine. */
	unknownServerIcon = $state<string | null>(null);
	/** The player's face, from their skin. Fetched separately so the window draws first. */
	playerHead = $state<string | null>(null);
	/** Their whole skin, which the sidebar's head is cut out of in three dimensions. */
	skinTexture = $state<string | null>(null);
	/** The one launch being prepared. Serial: two at once fight over the same blob store. */
	playing = $state<string | null>(null);

		/** The one launch being prepared. Serial: two at once fight over the same blob store. */
	running = $state<Record<string, "server" | "instance">>({});

	get onAServer() {
		return Object.values(this.running).includes("server");
	}
	error = $state<string | null>(null);
	/** Whether the stored lists have arrived. Before that, empty means "not read yet". */
	booted = $state(false);

	/** Servers this machine runs. Kept whole rather than merged into `servers`: one is an
	 * address somebody else owns, the other is a process this window can stop. */
	hosts = $state<HostedServer[]>([]);
	/** Which hosted server's console is open, if one is. */
	watching = $state<string | null>(null);
	/** What that console has said. Replaced wholesale when the drawer opens, appended after. */
	consoleLines = $state<string[]>([]);
	/** A newer release on GitHub, once the check has answered. */
	release = $state<Release | null>(null);
	/** Whether the player has dismissed the update banner for this run. */
	updateDismissed = $state(false);
	/** How the install is going: idle, then a fraction, then waiting to be restarted. */
	updating = $state(false);
	updateDone = $state(false);
	updateBytes = $state(0);
	updateTotal = $state<number | null>(null);
	updateError = $state<string | null>(null);

	/** The window is closing and waiting for the servers it started to save their worlds. */
	closing = $state(false);

	/** What the list is filtered to. One box, whichever list is showing. */
	filter = $state("");

	/** How much memory this machine has, so no picker may offer more than exists. */
	machineMemory = $state(0);

	/** How long everything has been played, keyed "server:<address>" / "instance:<id>". */
	playtime = $state<Record<string, Played>>({});
	/** Today where the player is, so a trend ends on today rather than on its last session. */
	today = $state(Math.floor(Date.now() / 86_400_000));

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
		// Caught, not swallowed by an unhandled rejection: without the whole skin the head
		// falls back to the flat face, and knowing why is worth a line in the log.
		void call<string | null>("skin_texture")
			.then((skin) => (this.skinTexture = skin))
			.catch((error) => console.error("skin_texture failed:", error));
		void this.loadVersions();
		// A hosted server that started or stopped is a row that has to change, and the process
		// that did it is not the one this window asked.
		await onHosts(() => void this.loadHosts());
		await onClosing(() => (this.closing = true));
		await onConsole((line: ConsoleLine) => {
			if (line.id !== this.watching) return;
			this.consoleLines = [...this.consoleLines, line.line].slice(-SCROLLBACK);
		});

		void this.loadHosts();
		void call<number>("machine_memory").then((mb) => (this.machineMemory = mb));
		void this.loadPlaytime();
		// Last, and never fatal: an out-of-date launcher still works, and GitHub being down
		// is not something to put on the screen.
		void call<Release | null>("check_update")
			.then((release) => (this.release = release))
			.catch(() => {});

		void call<Instance[]>("instances")
			.then((list) => (this.instances = list))
			// however it went, the lists are as full as they are going to get - an empty screen
			// may say so now
			.finally(() => (this.booted = true));
	}

	/* ---------------------------------------------------------------- playtime. */

	async loadPlaytime() {
		try {
			const [book, today] = await Promise.all([
				call<Record<string, Played>>("playtime"),
				call<number>("today")
			]);
			this.playtime = book ?? {};
			// The backend owns "today" because it owns the buckets; a webview left open across
			// midnight would otherwise draw the trend one day short.
			if (typeof today === "number") this.today = today;
		} catch {
			// A launcher with no playtime figures is a launcher, not a broken one.
		}
	}

	played(key: string): Played | undefined {
		return this.playtime[key];
	}

	/* ------------------------------------------------------------------ hosting. */

	async loadHosts() {
		try {
			this.hosts = await call<HostedServer[]>("hosts");
		} catch (error) {
			this.error = translate(String(error));
		}
	}

	/** Ask for the file, then build. Returns whether anything was made. */
	async pickPack(): Promise<string | null> {
		return pickPack(t("host.pickPack"));
	}

	async createHost(spec: {
		name: string;
		minecraft: string;
		loader: string | null;
		loaderVersion: string | null;
		port: number;
		memory: number;
		eula: boolean;
		publish: boolean;
		pack: string | null;
	}): Promise<boolean> {
		if (this.busyHosting) return false;
		this.busyHosting = true;
		this.error = null;
		// The same overlay a launch uses: importing a pack is the same four hundred megabytes.
		this.progress = { phase: t("host.building"), detail: spec.name, done: 0, total: 0 };
		try {
			this.hosts = await call<HostedServer[]>("create_host", {
				name: spec.name,
				minecraft: spec.minecraft,
				loader: spec.loader,
				loaderVersion: spec.loaderVersion,
				port: spec.port,
				memory: spec.memory,
				eula: spec.eula,
				publish: spec.publish,
				pack: spec.pack
			});
			return true;
		} catch (error) {
			this.error = describe(error);
			return false;
		} finally {
			this.busyHosting = false;
			this.progress = null;
		}
	}

	/** One build at a time: two at once would fight over the same downloads and the overlay. */
	busyHosting = $state(false);
	/** Which hosted server is mid start or stop, so its button can say so. */
	switching = $state<string | null>(null);

	async startHost(id: string) {
		if (this.switching) return;
		this.switching = id;
		this.error = null;
		try {
			await call("start_host", { id });
			this.watch(id);
		} catch (error) {
			this.error = describe(error);
		} finally {
			this.switching = null;
			await this.loadHosts();
		}
	}

	async stopHost(id: string) {
		if (this.switching) return;
		this.switching = id;
		try {
			await call("stop_host", { id });
		} catch (error) {
			this.error = describe(error);
		} finally {
			this.switching = null;
		}
	}

	/** The last resort. The card says what it costs before it offers this. */
	async killHost(id: string) {
		try {
			await call("kill_host", { id });
		} catch (error) {
			this.error = describe(error);
		}
		await this.loadHosts();
	}

	async removeHost(id: string, deleteFiles: boolean) {
		try {
			this.hosts = await call<HostedServer[]>("remove_host", { id, deleteFiles });
			if (this.watching === id) this.unwatch();
		} catch (error) {
			this.error = describe(error);
		}
	}

	/** Open the console on one server, filling it with whatever it has already said. */
	async watch(id: string) {
		this.watching = id;
		this.consoleLines = await call<string[]>("host_console", { id });
	}

	unwatch() {
		this.watching = null;
		this.consoleLines = [];
	}

	async sendCommand(id: string, line: string) {
		// Echoed straight away: a server takes a moment to answer, and a console that shows
		// nothing until it does reads as one that dropped the command.
		this.consoleLines = [...this.consoleLines, `> ${line}`].slice(-SCROLLBACK);
		try {
			await call("host_command", { id, line });
		} catch (error) {
			this.error = describe(error);
		}
	}

	async openHostDir(id: string) {
		try {
			await call("open_host_dir", { id });
		} catch (error) {
			this.error = describe(error);
		}
	}

	/** The mod browser, pointed at a server this machine runs rather than one on the network. */
	openHostMods(server: HostedServer) {
		this.openMods({
			id: server.id,
			name: server.name,
			address: `host-${server.id}`,
			loader: server.loader,
			minecraft: server.minecraft
		});
	}

	/* ------------------------------------------------------------------ updates. */

	/**
	 * Fetch the new version and write it over this one.
	 *
	 * The updater verifies the signature on what it downloads against the public key built
	 * into this binary, so a release that was not signed with the matching private key is
	 * refused rather than installed.
	 */
	async installUpdate() {
		if (this.updating || this.updateDone) return;
		this.updating = true;
		this.updateError = null;
		this.updateBytes = 0;
		this.updateTotal = null;
		try {
			const did = await downloadAndInstall((bytes, total) => {
				this.updateBytes = bytes;
				this.updateTotal = total;
			});
			if (!did) {
				// The banner said there was one; the updater disagrees. Its manifest is the
				// authority - the banner only compares tag names.
				this.updateError = t("update.notOffered");
				return;
			}
			this.updateDone = true;
		} catch (error) {
			this.updateError = translate(String(error));
		} finally {
			this.updating = false;
		}
	}

	async restart() {
		await relaunch();
	}

	/** The release page, for anyone who would rather read it than press a button. */
	async openRelease() {
		if (!this.release) return;
		try {
			await call("open_release", { url: this.release.url });
		} catch (error) {
			this.error = translate(String(error));
		}
	}

	/**
	 * Switch halves. The mod browser draws over the list behind it, so leaving it open would
	 * mean pressing Servers and still looking at an instance's mods.
	 */
	show(view: View) {
		this.view = view;
		// A filter typed for one list would silently empty the next one.
		this.filter = "";
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
	 * Choose a loader for a server that runs none. Client-side mods do not need the server to
	 * know about them.
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
		// Not blocked by a server session: singleplayer needs no account on any server, and
		// the game is happy to run twice.
		if (this.playing || this.running[instance.id]) return;
		await this.launch(instance.id, "instance", instance.name, () =>
			call("play_instance", { id: instance.id })
		);
	}

	/** The server whose profile question is open, if one is. */
	choosing = $state<SavedServer | null>(null);

	/**
	 * Ask before starting, unless the server has already answered. A server that publishes a
	 * pack has decided what runs; anything else leaves the choice open.
	 */
	askThenPlay(server: SavedServer) {
		if (this.playing || this.running[server.id]) return;
		const manifest = this.status[server.id]?.manifest;
		if (manifest && manifest.mods.length > 0) {
			void this.play(server);
			return;
		}
		this.choosing = server;
	}

	async play(server: SavedServer, instance: string | null = null) {
		if (this.playing || this.running[server.id]) return;
		if (this.onAServer) {
			this.error = t("error.alreadyOnAServer");
			return;
		}
		this.choosing = null;
		await this.launch(server.id, "server", server.name, () =>
			call("play", { address: server.address, instance })
		);
	}

	/**
	 * Everything a launch has in common: overlay up while preparing, gone the moment the window
	 * appears, row marked until the game exits.
	 */
	async launch(id: string, kind: "server" | "instance", label: string, start: () => Promise<unknown>) {
		this.playing = id;
		this.error = null;
		this.progress = { phase: t("progress.prepare"), detail: label, done: 0, total: 0 };

		const stop = await onRunning(() => {
			this.progress = null;
			this.playing = null;
			this.running[id] = kind;
		});
		try {
			await start();
		} catch (error) {
			this.error = describe(error);
		} finally {
			stop();
			delete this.running[id];
			if (this.playing === id) this.playing = null;
			this.progress = null;
			// The session's length is only known now, and it is what the row above will show.
			void this.loadPlaytime();
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
	/** What the server lets a player add on top of what it ships. All four until it says otherwise. */
	allowedKinds = $state<Kind[]>([...KINDS]);
	hits = $state<Hit[]>([]);
	loadingMods = $state(false);
	note = $state("");
	/** How the catalogue is ordered. Downloads by default; relevance means nothing empty. */
	sort = $state<Sort>("downloads");
	/** The one row being installed or removed right now, so only its own button spins. */
	working = $state<string | null>(null);
	/** Narrows the installed list. Client-side: it is a folder, not a catalogue. */
	installedFilter = $state("");

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
		this.note = "";
		this.sort = "downloads";
		this.installedFilter = "";
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
		this.installedFilter = "";
		await this.loadMods("");
	}

	async setSort(sort: Sort) {
		if (sort === this.sort) return;
		this.sort = sort;
		await this.loadMods(this.query);
	}

	async switchKind(kind: Kind) {
		this.kind = kind;
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
				offset: this.#offset,
				sort: this.sort
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

	/**
	 * Install one, now.
	 *
	 * A row and its button rather than a selection and a footer: the old flow had one
	 * highlight meaning "will be installed" on one tab and "will be deleted" on another, and
	 * no way to tell them apart but the colour of a button somewhere else.
	 */
	async install(hit: Hit) {
		if (!this.browsing || this.working) return;
		this.working = hit.id;
		this.note = "";
		try {
			const added = await call<string[]>("install_mods", {
				address: this.browsing.address,
				kind: this.kind,
				ids: [hit.id]
			});
			// Zero is not success: the catalogue had nothing for this version and said so by
			// handing back an empty list.
			if (added.length === 0) {
				this.note = t("mods.installedNone");
				return;
			}
			// What arrived beyond what was asked for. Iris without Sodium is a crash, so the
			// backend pulls dependencies - and the player should be told it did.
			this.note =
				added.length > 1
					? t("mods.installedWithDeps", { name: hit.title, count: added.length - 1 })
					: t("mods.installedOne", { name: hit.title });
			this.markInstalled(hit.id);
		} catch (error) {
			this.note = translate(String(error));
		} finally {
			this.working = null;
		}
	}

	/** Take one away again. Named apart from `remove`, which removes a whole server. */
	async uninstall(hit: Hit) {
		if (!this.browsing || this.working) return;
		this.working = hit.id;
		this.note = "";
		try {
			const gone = await call<string[]>("remove_mods", {
				address: this.browsing.address,
				kind: this.kind,
				names: [hit.id]
			});
			if (gone.length === 0) {
				this.note = t("mods.removedNone");
				return;
			}
			this.note = t("mods.removedOne", { name: hit.title });
			this.hits = this.hits.filter((row) => row.id !== hit.id);
		} catch (error) {
			this.note = translate(String(error));
		} finally {
			this.working = null;
		}
	}

	/**
	 * Mark the row done without refetching. Reloading the whole catalogue to change one badge
	 * loses the player's place in a list they were reading.
	 */
	markInstalled(id: string) {
		this.hits = this.hits.map((hit) => (hit.id === id ? { ...hit, installed: true } : hit));
	}

	/**
	 * Signing in, and what the player has to do while it waits. The prompt arrives as an event
	 * because the call does not return until they have finished in their browser.
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
	 * The skin, from the webview's own file picker. Both answer with null on success and the
	 * reason otherwise, so the dialog can show it where the player is looking.
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
			this.skinTexture = await call<string | null>("skin_texture");
		} catch {
		// a stale face is better than none, and the change itself already reported
		}
	}

	async saveSettings(next: Settings) {
		this.settings = await call<Settings>("save_settings", { settings: next });
		this.applyLanguage();
	}

	/** Put the chosen language and theme into effect. Every screen reads both through them. */
	applyLanguage() {
		locale.current = resolveLocale(this.settings.language ?? "system");
		applyTheme(this.settings);
	}
}

export const launcher = new LauncherState();
