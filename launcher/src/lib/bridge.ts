import type { Progress, SignInPrompt } from "./types";

/**
 * The Rust side, or nothing.
 *
 * Running under `vite dev` in a plain browser there is no Tauri to call. Rather than let
 * every screen guard for that, the bridge answers with sample data of the same shape - so
 * layout work does not need a Rust build, and a missing backend can never look like an
 * empty server list.
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
					language: "system", memory: 0, offline_name: "Player", manifest_port: 25673,
					keep_open: true, curseforge_key: "", jvm_profile: "balanced", jvm_args: ""
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
		case "allowed_kinds":
			return String(args.address).startsWith("194.54.88.19")
				? ["mod", "resourcepack", "shader"]
				: ["mod", "resourcepack", "shader", "datapack"];
		case "mods": {
			const tab = String(args.tab);
			const kind = String(args.kind ?? "mod");
			const suffix = kind === "mod" ? ".jar" : ".zip";
			if (tab === "pack") {
				const shipped = shippedByKind[kind] ?? 0;
				return Array.from({ length: shipped }, (_, i) => ({
					id: `${kind}-${i}`, title: `${kind}-${i}${suffix}`, description: "",
					downloads: 0, source: "pack", on_server: true, installed: false, icon: null
				}));
			}
			if (tab === "installed") {
				return kind === "mod"
					? [
							{ id: "simplerpc.jar", title: "SimpleRPC-4.1.4.jar", description: "in deinem Profil",
							  downloads: 0, source: "profile", on_server: false, installed: false, icon: null }
						]
					: [
							{ id: `eigenes-${kind}${suffix}`, title: `Eigenes ${kind}${suffix}`,
							  description: "in deinem Profil", downloads: 0, source: "profile",
							  on_server: false, installed: false, icon: null }
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
		case "versions":
			return ["1.21.11", "1.21.10", "1.21.8", "1.21.1", "1.20.6", "1.20.1", "1.16.5", "1.8.9"];
		case "player_head":
			// Eight pixels of face, inline: the sidebar and the skin dialog both draw it, and
			// neither is testable against a null.
			return "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAO0lEQVR42mOwUZPEihjwSWypCEBDIAkg9ezZM5uKExAhOJsBrqonygaI4FwGZFFkOahElI0GHKFIYCIALxxKRxZbJi4AAAAASUVORK5CYII=";
		default:
			return null;
	}
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
};

/** One page per category, so every category shows something without a Rust build. */
const catalogues: Record<string, SampleHit[]> = {
	mod: [
		{ id: "sodium", title: "Sodium", description: "Moderne Rendering-Engine, deutlich mehr FPS",
		  downloads: 42_000_000, source: "modrinth", on_server: true, installed: false,
		  icon: "https://cdn.modrinth.com/data/AANobbMI/icon.png" },
		{ id: "iris", title: "Iris Shaders", description: "Shader-Unterstützung, kompatibel mit Sodium",
		  downloads: 28_000_000, source: "modrinth", on_server: false, installed: false,
		  icon: "https://cdn.modrinth.com/data/YL57xq9U/icon.png" },
		{ id: "cf:263420", title: "Xaero's Minimap", description: "Karte in der Ecke",
		  downloads: 241_000_000, source: "curseforge", on_server: true, installed: false,
		  icon: "https://media.forgecdn.net/avatars/thumbnails/175/905/64/64/636426383151327212.png" },
		{ id: "chat-heads", title: "Chat Heads", description: "Zeigt den Kopf des Absenders im Chat",
		  downloads: 9_400_000, source: "modrinth", on_server: false, installed: false,
		  icon: "https://cdn.modrinth.com/data/Wb5oqrBJ/icon.png" },
		{ id: "appleskin", title: "AppleSkin", description: "Zeigt Sättigung und Nährwerte an",
		  downloads: 31_000_000, source: "modrinth", on_server: false, installed: false,
		  icon: "https://cdn.modrinth.com/data/EsAfCjCV/icon.png" },
		{ id: "jei", title: "Just Enough Items", description: "Rezepte nachschlagen",
		  downloads: 88_000_000, source: "modrinth", on_server: false, installed: false,
		  icon: "https://cdn.modrinth.com/data/u6dRKJwZ/icon.png" }
	],
	resourcepack: [
		{ id: "faithful", title: "Faithful 32x", description: "Vanilla, nur doppelt so scharf",
		  downloads: 12_000_000, source: "modrinth", on_server: false, installed: false, icon: null },
		{ id: "cf:409223", title: "Stay True", description: "Vanilla-Optik, weichere Texturen",
		  downloads: 3_100_000, source: "curseforge", on_server: false, installed: false, icon: null },
		{ id: "bare-bones", title: "Bare Bones", description: "Flach und ohne Rauschen, wie im Trailer",
		  downloads: 2_400_000, source: "modrinth", on_server: false, installed: false, icon: null }
	],
	shader: [
		{ id: "complementary-unbound", title: "Complementary Unbound",
		  description: "Vielseitig, läuft auch auf schwächeren Karten",
		  downloads: 21_000_000, source: "modrinth", on_server: false, installed: false, icon: null },
		{ id: "bsl", title: "BSL Shaders", description: "Warme Farben, sehr verbreitet",
		  downloads: 15_000_000, source: "modrinth", on_server: false, installed: false, icon: null },
		{ id: "cf:429026", title: "Sildur's Vibrant Shaders", description: "Von sparsam bis extrem",
		  downloads: 9_800_000, source: "curseforge", on_server: false, installed: false, icon: null }
	],
	datapack: [
		{ id: "terralith", title: "Terralith", description: "Über hundert neue Biome, ohne Mod",
		  downloads: 7_600_000, source: "modrinth", on_server: false, installed: false, icon: null },
		{ id: "incendium", title: "Incendium", description: "Neuer Nether mit eigenen Strukturen",
		  downloads: 2_900_000, source: "modrinth", on_server: false, installed: false, icon: null },
		{ id: "vanilla-tweaks", title: "Vanilla Tweaks", description: "Kleine Rezepte und Komfort",
		  downloads: 1_800_000, source: "modrinth", on_server: false, installed: false, icon: null }
	]
};

function sampleInstances() {
	return [
		{ id: "a1", name: "Create Astral", minecraft: "1.18.2", loader: "forge",
		  loader_version: "1.18.2-40.2.0", source: "mrpack" },
		{ id: "b2", name: "Sodium Test", minecraft: "1.21.1", loader: "neoforge",
		  loader_version: null, source: null }
	];
}
