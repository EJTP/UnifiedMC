import type { ConsoleLine, Progress, SignInPrompt } from "./types";

/**
 * The Rust side, or nothing. Under `vite dev` there is no Tauri, so the bridge answers with
 * sample data of the same shape - a missing backend can never look like an empty server list.
 */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
	if (!inTauri) {
		return sample(command, args) as T;
	}
	const { invoke } = await import("@tauri-apps/api/core");
	return invoke<T>(command, args);
}

/** The device code, emitted once the sign-in has one and before it waits for the player. */
export async function onSignIn(
	handler: (prompt: SignInPrompt) => void
): Promise<() => void> {
	if (!inTauri) {
		return () => {};
	}
	const { listen } = await import("@tauri-apps/api/event");
	return listen<SignInPrompt>("unifiedmc://signin", (event) => handler(event.payload));
}

/** The game's window is up. The command that started it runs until the game ends. */
export async function onRunning(handler: () => void): Promise<() => void> {
	if (!inTauri) {
		return () => {};
	}
	const { listen } = await import("@tauri-apps/api/event");
	return listen("unifiedmc://running", () => handler());
}

/** One line out of a hosted server's console, as it prints it. */
export async function onConsole(handler: (line: ConsoleLine) => void): Promise<() => void> {
	if (!inTauri) {
		return () => {};
	}
	const { listen } = await import("@tauri-apps/api/event");
	return listen<ConsoleLine>("unifiedmc://console", (event) => handler(event.payload));
}

/** A hosted server started or stopped. The list has to be read again; the event carries no
 * payload, because "which one" is not enough to redraw a row that also has a player list. */
export async function onHosts(handler: () => void): Promise<() => void> {
	if (!inTauri) {
		return () => {};
	}
	const { listen } = await import("@tauri-apps/api/event");
	return listen("unifiedmc://hosts", () => handler());
}

/**
 * The update itself: check, download, install, relaunch.
 *
 * Everything here is the updater plugin's, not ours - it verifies the signature on what it
 * downloads against the public key compiled into the app, which is the whole reason an
 * in-place update is safe to offer at all. Outside Tauri it reports "nothing to do", because
 * a browser has no binary to replace.
 */
export async function downloadAndInstall(
	onProgress: (downloaded: number, total: number | null) => void
): Promise<boolean> {
	if (!inTauri) {
		return false;
	}
	const { check } = await import("@tauri-apps/plugin-updater");
	const update = await check();
	if (!update) {
		return false;
	}

	let downloaded = 0;
	let total: number | null = null;
	await update.downloadAndInstall((event) => {
		if (event.event === "Started") {
			total = event.data.contentLength ?? null;
			onProgress(0, total);
		} else if (event.event === "Progress") {
			downloaded += event.data.chunkLength;
			onProgress(downloaded, total);
		} else if (event.event === "Finished") {
			onProgress(total ?? downloaded, total);
		}
	});
	return true;
}

/** Restart into the version that was just written. */
export async function relaunch(): Promise<void> {
	if (!inTauri) return;
	const { relaunch: restart } = await import("@tauri-apps/plugin-process");
	await restart();
}

/**
 * The window was asked to close while servers were still up, and is waiting for them to save.
 * There is no cancelling it: the alternative is a corrupt world.
 */
export async function onClosing(handler: () => void): Promise<() => void> {
	if (!inTauri) {
		return () => {};
	}
	const { listen } = await import("@tauri-apps/api/event");
	return listen("unifiedmc://closing", () => handler());
}

/**
 * The native file picker. A modpack is hundreds of megabytes, so what crosses the bridge is
 * the path - a webview `<input type="file">` would hand over the bytes.
 */
export async function pickPack(title: string): Promise<string | null> {
	if (!inTauri) {
		return "/home/spieler/Downloads/All of Create 3.4.mrpack";
	}
	const { open } = await import("@tauri-apps/plugin-dialog");
	const picked = await open({
		title,
		multiple: false,
		directory: false,
		filters: [{ name: "Modpack", extensions: ["mrpack", "zip"] }]
	});
	return typeof picked === "string" ? picked : null;
}

/** The same picker, for the one binary a player may have to name themselves. */
export async function pickJava(title: string): Promise<string | null> {
	if (!inTauri) {
		return "/usr/lib/jvm/java-21-openjdk/bin/java";
	}
	const { open } = await import("@tauri-apps/plugin-dialog");
	const picked = await open({ title, multiple: false, directory: false });
	return typeof picked === "string" ? picked : null;
}

/**
 * The last question before a directory leaves the disk. The OS modal rather than one of our
 * own dialogs: this one has to survive a misplaced click, and a card-sized dialog is
 * dismissed by the same stray Escape that closes everything else in the window. The labels
 * come from the caller - the bridge does not know the language.
 */
export async function confirmDelete(message: string, okLabel: string, cancelLabel: string): Promise<boolean> {
	if (!inTauri) return true;
	const { confirm } = await import("@tauri-apps/plugin-dialog");
	return confirm(message, { kind: "warning", okLabel, cancelLabel });
}

export async function onProgress(handler: (progress: Progress) => void): Promise<() => void> {
	if (!inTauri) {
		return () => {};
	}
	const { listen } = await import("@tauri-apps/api/event");
	const stop = await listen<Progress>("unifiedmc://progress", (event) => handler(event.payload));
	return stop;
}

function sample(command: string, args: Record<string, unknown>): unknown {
	switch (command) {
		case "bootstrap":
			return {
				servers: [
					{ id: "1", name: "All of Create Aeronautics", address: "194.54.88.19:25601", loader: null, minecraft: null },
					{ id: "2", name: "Paper", address: "mc.example.com", loader: "fabric", minecraft: null },
					{ id: "3", name: "Testserver", address: "10.0.0.5:25565", loader: null, minecraft: null },
					{ id: "4", name: "Vanilla Survival", address: "play.example.net", loader: null, minecraft: null }
				],
				settings: {
					language: "system", memory: 0, offline_name: "Player",
					keep_open: true, jvm_profile: "balanced", jvm_args: "", accent: "violet",
					accent_primary: "#7c3aed", accent_cta: "#f43f5e", backdrop: "midnight",
					java_path: ""
				},
				session: { name: "EJTP", uuid: "0", kind: "microsoft" },
				unknown_server_icon: null
			};
		case "probe": {
			const id = String(args.id);

			if (id === "3") {
				return { id, online: false, error: "connection timed out", motd: [], icon: null, players: 0, max_players: 0, manifest: null };
			}
			// Vanilla: reachable, announces a version and no loader at all - the case where the
			// player has to be asked what to run, and where the setup dialog earns its place.
			if (id === "4") {
				return {
					id, online: true, error: null,
					motd: [{ text: "Survival, seit 2019" }],
					icon: null, players: 7, max_players: 40,
					manifest: { minecraft: "1.21.8", loader: null, mods: [], config: [] }
				};
			}
			const modded = id === "1";
			return {
				id,
				online: true,
				error: null,
				motd: modded
					? [
							{ text: "Create ", color: "#FFAA00", bold: true },
							{ text: "Aeronautics", color: "#55FFFF" }
						]
					: [{ text: "A Minecraft Server" }],
				icon: null,
				players: modded ? 3 : 12,
				max_players: modded ? 20 : 60,
				manifest: {
					minecraft: modded ? "1.21.1" : "1.21.11",
					loader: modded
						? { type: "neoforge", version: "21.1.247" }
						: { type: "fabric", version: "0.19.3" },
					mods: Array.from({ length: modded ? 214 : 0 }, (_, i) => ({
						name: `mod-${i}.jar`, sha1: String(i), url: ""
					})),
					config: Array.from({ length: modded ? 410 : 0 }, (_, i) => ({
						path: `c${i}.toml`, sha1: String(i), url: "", force: false
					}))
				}
			};
		}
		// The one server with a pack of its own owns the world data, so datapacks are not on
		// offer there - the same shape the Rust side answers with.
		case "allowed_kinds": {
			const at = String(args.address);
			// the pack server decides; an instance with no loader cannot run a mod at all
			if (at.startsWith("194.54.88.19")) return ["mod", "resourcepack", "shader"];
			if (at === "instance-c3") return ["resourcepack", "shader", "datapack"];
			// A server the player runs owns its own world, so all four are theirs to add to.
			if (at === "host-h2") return ["resourcepack", "shader", "datapack"];
			return ["mod", "resourcepack", "shader", "datapack"];
		}
		case "mods": {
			const tab = String(args.tab);
			const kind = String(args.kind ?? "mod");
			const suffix = kind === "mod" ? ".jar" : ".zip";
			// One list of what is actually here: the server's first, then the player's own.
			if (tab === "installed") {
				const shipped = shippedByKind[kind] ?? 0;
				return [
					...Array.from({ length: shipped }, (_, i) => ({
						id: `${kind}-${i}`, title: `${kind}-${i}${suffix}`, description: "",
						downloads: 0, source: "pack", on_server: true, installed: false, icon: null,
						author: "", version: "1.4.2", url: "", removable: false
					})),
					{
						id: kind === "mod" ? "simplerpc.jar" : `eigenes-${kind}${suffix}`,
						title: kind === "mod" ? "SimpleRPC" : `Eigenes ${kind}`,
						description: "", downloads: 0, source: "profile",
						on_server: false, installed: true, icon: null,
						author: "", version: "4.1.4", url: "", removable: true
					}
				];
			}
			const page = Number(args.offset ?? 0) / 40;
			const catalogue = catalogues[kind] ?? catalogues.mod;
			return catalogue.map((hit, i) => ({ ...hit, id: page ? `${hit.id}-${page}-${i}` : hit.id,
				title: page ? `${hit.title} (Seite ${page + 1})` : hit.title }));
		}
		case "install_mods":
			return String(args.kind ?? "mod") === "mod"
				? ["iris-neoforge.jar", "sodium-neoforge.jar"]
				: [`${args.kind}-1.zip`];
		case "remove_mods":
			return args.names;
		case "sign_in":
			// The browser has no device flow to run; answer as though the player finished it,
			// so the signed-in half of every screen can be looked at without Tauri.
			return { name: "EJTP", uuid: "069a79f4-44e9-4726-a5be-fca90e38aaf5", kind: "microsoft" };
		case "sign_out":
			return { name: "Player", uuid: "0", kind: "offline" };
		case "set_skin":
		case "reset_skin":
			return null;
		case "instances":
			return sampleInstances();
		// Appends rather than answering with an empty list, because the caller reads the new
		// instance back as the last entry - an empty answer reads as "creation did nothing".
		case "add_instance":
			return [
				...sampleInstances(),
				{
					id: "new",
					name: String(args.name || `Minecraft ${args.minecraft}`),
					minecraft: String(args.minecraft ?? ""),
					loader: (args.loader as string | null) ?? null,
					loader_version: (args.loaderVersion as string | null) ?? null,
					source: null
				}
			];
		case "remove_instance":
			return sampleInstances();
		case "loader_versions": {
			const loader = String(args.loader);
			if (loader === "fabric") return ["0.19.3", "0.19.2", "0.19.1", "0.18.6"];
			if (loader === "neoforge") return ["21.1.248", "21.1.247", "21.1.244"];
			if (loader === "forge") return ["1.21.1-52.1.16", "1.21.1-52.1.15"];
			if (loader === "quilt") return ["0.30.1-beta.3", "0.30.0"];
			return [];
		}
		case "today":
			return Math.floor(Date.now() / 86_400_000);
		case "playtime": {
			// Enough shape to look at the trend without a Rust build: one server played most
			// evenings, one instance played once, and one row that has never been touched.
			const today = Math.floor(Date.now() / 86_400_000);
			const spread = (pattern: number[]) =>
				Object.fromEntries(
					pattern.map((hours, i) => [today - (pattern.length - 1 - i), Math.round(hours * 3600)])
				);
			return {
				"server:194.54.88.19:25601": {
					seconds: 184 * 3600,
					last: Math.floor(Date.now() / 1000) - 3600 * 5,
					sessions: 96,
					days: spread([0, 1.5, 3.2, 0, 2.1, 4.8, 1.2, 0, 0.6, 2.9, 5.4, 3.1, 0, 2.2])
				},
				"server:play.example.net": {
					seconds: 12 * 3600,
					last: Math.floor(Date.now() / 1000) - 86_400 * 9,
					sessions: 7,
					days: spread([0, 0, 1.1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
				},
				"instance:a1": {
					seconds: 3 * 3600 + 1500,
					last: Math.floor(Date.now() / 1000) - 86_400 * 2,
					sessions: 3,
					days: spread([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1.2, 0, 0])
				}
			};
		}
		case "machine_memory":
			return 16384;
		case "data_dir":
			return "/home/spieler/.unifiedmc";
		case "jvm_preview":
			// No -Xms: lyceris only ever emits -Xmx, and a preview that shows a flag the
			// launcher never passes is worse than no preview.
			return [
				"-Xmx6144M", "-XX:+UseG1GC", "-XX:MaxGCPauseMillis=50",
				"-XX:G1HeapRegionSize=16M", "-XX:+UnlockExperimentalVMOptions",
				"-XX:+DisableExplicitGC", "-Dfml.ignoreInvalidMinecraftCertificates=true"
			];
		case "save_settings":
			return args.settings;
		case "hosts":
			return sampleHosts();
		case "create_host":
			return [
				...sampleHosts(),
				{
					id: "new",
					name: String(args.name || `Minecraft ${args.minecraft}`),
					minecraft: String(args.minecraft ?? "1.21.8"),
					loader: (args.loader as string | null) ?? null,
					loader_version: null,
					port: Number(args.port ?? 25565),
					memory: Number(args.memory ?? 4096),
					source: args.pack ? "All of Create 3.4" : null,
					command: ["java", "-jar", "server.jar"],
					eula: true,
					publishes: Boolean(args.publish) && Boolean(args.loader),
					running: false,
					players: [],
					address: `localhost:${args.port ?? 25565}`,
					directory: `/home/spieler/.unifiedmc/hosted/new`
				}
			];
		case "start_host":
		case "stop_host":
		case "kill_host":
		case "host_command":
		case "open_host_dir":
		case "open_release":
			return null;
		case "remove_host":
			return sampleHosts().filter((server) => server.id !== args.id);
		case "host_console":
			return [
				"[12:00:01] [main/INFO]: Starting minecraft server version 1.21.1",
				"[12:00:02] [main/INFO]: Loading properties",
				"[12:00:09] [Server thread/INFO]: Preparing level \"world\"",
				"[12:00:21] [Server thread/INFO]: Done (12.482s)! For help, type \"help\"",
				"[12:03:44] [Server thread/INFO]: EJTP joined the game"
			];
		case "check_update":
			// The banner is half the feature; without this it can never be looked at.
			return {
				current: "0.1.7",
				latest: "0.1.8",
				url: "https://github.com/EJTP/UnifiedMC/releases/tag/v0.1.8",
				notes: [
					"## Run a server from the launcher",
					"Drop in a `.mrpack` or pick a version and a loader. Java is not a prerequisite.",
					"| | |",
					"|---|---|",
					"| Windows | `.msi`, or the `-setup.exe` to double-click |",
					"| macOS | one universal `.dmg` - Intel and Apple Silicon, same file |",
					"The macOS build is not signed, so the first open needs right-click then Open, or",
					"`xattr -dr com.apple.quarantine /Applications/UnifiedMC.app`",
					"https://github.com/EJTP/UnifiedMC/releases/tag/v0.1.11"
				].join("\n")
			};
		case "versions":
			return ["1.21.11", "1.21.10", "1.21.8", "1.21.1", "1.20.6", "1.20.1", "1.16.5", "1.8.9"];
		case "skin_texture":
			// The default Steve, 64x64, so the sidebar's head can be looked at without a Rust
			// build. Drawn here rather than fetched: nothing in a browser fallback may need
			// the network to render.
			return steveSkin();
		case "player_head":
			// Eight pixels of face, inline: the sidebar and the skin dialog both draw it, and
			// neither is testable against a null.
			return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAO0lEQVR42mOwUZPEihjwSWypCEBDIAkg9ezZM5uKExAhOJsBrqonygaI4FwGZFFkOahElI0GHKFIYCIALxxKRxZbJi4AAAAASUVORK5CYII=";
		default:
			return null;
	}
}

/**
 * A minimal stand-in skin: a 64x64 png with the head squares filled in. Not Mojang's texture -
 * that one belongs to Mojang and is read from the client jar on a real machine - just enough
 * coloured squares in the right places to prove the cube is cut correctly.
 */
function steveSkin(): string {
	if (typeof document === "undefined") return "";
	const canvas = document.createElement("canvas");
	canvas.width = 64;
	canvas.height = 64;
	const paint = canvas.getContext("2d");
	if (!paint) return "";

	// base head: right, front, left, back on row 8; top and bottom on row 0
	const base: [number, number, string][] = [
		[0, 8, "#8a6244"], [8, 8, "#b2836a"], [16, 8, "#8a6244"], [24, 8, "#7a5540"],
		[8, 0, "#3f2a1d"], [16, 0, "#6b4a36"]
	];
	for (const [x, y, colour] of base) {
		paint.fillStyle = colour;
		paint.fillRect(x, y, 8, 8);
	}
	// The overlay layer, 32 to the right. Mostly transparent, the way a real skin's is - a
	// solid one would cover the face entirely and prove nothing about the layering.
	paint.fillStyle = "#2f6f4f";
	paint.fillRect(40, 0, 8, 8); // the top of the hat
	for (const [x] of [[32], [40], [48], [56]]) {
		paint.fillRect(x, 8, 8, 2); // a brim, two pixels deep, all the way round
	}
	// two eyes, so it is obvious which way the face is pointing
	paint.fillStyle = "#ffffff";
	paint.fillRect(10, 12, 2, 2);
	paint.fillRect(14, 12, 2, 2);
	return canvas.toDataURL("image/png");
}

/** How much of each category the sample pack server ships. */
const shippedByKind: Record<string, number> = { mod: 40, resourcepack: 2, shader: 0, datapack: 3 };

type SampleHit = {
	id: string;
	title: string;
	description: string;
	downloads: number;
	source: string;
	on_server: boolean;
	installed: boolean;
	icon: string | null;
	author: string;
	version: string;
	url: string;
	removable: boolean;
};

/** One page per category, so every category shows something without a Rust build. */
const catalogues: Record<string, SampleHit[]> = {
	mod: [
		{ id: "sodium", title: "Sodium", description: "Moderne Rendering-Engine, deutlich mehr FPS",
		  downloads: 42_000_000, source: "modrinth", on_server: true, installed: false,
		  icon: "https://cdn.modrinth.com/data/AANobbMI/icon.png" , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "iris", title: "Iris Shaders", description: "Shader-Unterstützung, kompatibel mit Sodium",
		  downloads: 28_000_000, source: "modrinth", on_server: false, installed: false,
		  icon: "https://cdn.modrinth.com/data/YL57xq9U/icon.png" , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "cf:263420", title: "Xaero's Minimap", description: "Karte in der Ecke",
		  downloads: 241_000_000, source: "curseforge", on_server: true, installed: false,
		  icon: "https://media.forgecdn.net/avatars/thumbnails/175/905/64/64/636426383151327212.png" , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "chat-heads", title: "Chat Heads", description: "Zeigt den Kopf des Absenders im Chat",
		  downloads: 9_400_000, source: "modrinth", on_server: false, installed: false,
		  icon: "https://cdn.modrinth.com/data/Wb5oqrBJ/icon.png" , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "appleskin", title: "AppleSkin", description: "Zeigt Sättigung und Nährwerte an",
		  downloads: 31_000_000, source: "modrinth", on_server: false, installed: false,
		  icon: "https://cdn.modrinth.com/data/EsAfCjCV/icon.png" , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "jei", title: "Just Enough Items", description: "Rezepte nachschlagen",
		  downloads: 88_000_000, source: "modrinth", on_server: false, installed: false,
		  icon: "https://cdn.modrinth.com/data/u6dRKJwZ/icon.png" , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false }
	],
	resourcepack: [
		{ id: "faithful", title: "Faithful 32x", description: "Vanilla, nur doppelt so scharf",
		  downloads: 12_000_000, source: "modrinth", on_server: false, installed: false, icon: null , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "cf:409223", title: "Stay True", description: "Vanilla-Optik, weichere Texturen",
		  downloads: 3_100_000, source: "curseforge", on_server: false, installed: false, icon: null , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "bare-bones", title: "Bare Bones", description: "Flach und ohne Rauschen, wie im Trailer",
		  downloads: 2_400_000, source: "modrinth", on_server: false, installed: false, icon: null , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false }
	],
	shader: [
		{ id: "complementary-unbound", title: "Complementary Unbound",
		  description: "Vielseitig, läuft auch auf schwächeren Karten",
		  downloads: 21_000_000, source: "modrinth", on_server: false, installed: false, icon: null , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "bsl", title: "BSL Shaders", description: "Warme Farben, sehr verbreitet",
		  downloads: 15_000_000, source: "modrinth", on_server: false, installed: false, icon: null , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "cf:429026", title: "Sildur's Vibrant Shaders", description: "Von sparsam bis extrem",
		  downloads: 9_800_000, source: "curseforge", on_server: false, installed: false, icon: null , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false }
	],
	datapack: [
		{ id: "terralith", title: "Terralith", description: "Über hundert neue Biome, ohne Mod",
		  downloads: 7_600_000, source: "modrinth", on_server: false, installed: false, icon: null , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "incendium", title: "Incendium", description: "Neuer Nether mit eigenen Strukturen",
		  downloads: 2_900_000, source: "modrinth", on_server: false, installed: false, icon: null , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false },
		{ id: "vanilla-tweaks", title: "Vanilla Tweaks", description: "Kleine Rezepte und Komfort",
		  downloads: 1_800_000, source: "modrinth", on_server: false, installed: false, icon: null , author: "somebody", version: "1.0.0", url: "https://modrinth.com/mod/x", removable: false }
	]
};

function sampleHosts() {
	return [
		{
			id: "h1",
			name: "Create Aeronautics",
			minecraft: "1.21.1",
			loader: "neoforge",
			loader_version: "21.1.247",
			port: 25565,
			memory: 6144,
			source: "All of Create Aeronautics 3.4",
			command: ["java", "-Xmx6144M", "@libraries/.../unix_args.txt", "nogui"],
			eula: true,
			publishes: true,
			running: true,
			players: ["EJTP", "Alex"],
			address: "localhost:25565",
			directory: "/home/spieler/.unifiedmc/hosted/h1"
		},
		// Vanilla, stopped, and publishing nothing: the other half of every state a row has.
		{
			id: "h2",
			name: "Survival",
			minecraft: "1.21.8",
			loader: null,
			loader_version: null,
			port: 25575,
			memory: 4096,
			source: null,
			command: ["java", "-Xmx4096M", "-jar", "server.jar", "nogui"],
			eula: true,
			publishes: false,
			running: false,
			players: [],
			address: "localhost:25575",
			directory: "/home/spieler/.unifiedmc/hosted/h2"
		}
	];
}

function sampleInstances() {
	return [
		{ id: "a1", name: "Create Astral", minecraft: "1.18.2", loader: "forge",
		  loader_version: "1.18.2-40.2.0", source: "mrpack" },
		{ id: "b2", name: "Sodium Test", minecraft: "1.21.1", loader: "neoforge",
		  loader_version: null, source: null },
		// No loader: nothing here can run a mod, which is a state the browser has to show
		{ id: "c3", name: "Vanilla 1.21.8", minecraft: "1.21.8", loader: null,
		  loader_version: null, source: null }
	];
}
