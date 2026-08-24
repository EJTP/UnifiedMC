import type { Progress } from "./types";

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
					{ id: "1", name: "All of Create Aeronautics", address: "194.54.88.19:25601" },
					{ id: "2", name: "Vanilla", address: "mc.example.com" },
					{ id: "3", name: "Testserver", address: "10.0.0.5:25565" }
				],
				settings: { memory: 0, offline_name: "Player", manifest_port: 25673, keep_open: true },
				session: { name: "EJTP", uuid: "0", token: "t", kind: "microsoft" }
			};
		case "probe": {
			const id = String(args.id);
			if (id === "3") {
				return { id, online: false, error: "connection timed out", motd: "", players: 0, max_players: 0, manifest: null };
			}
			const modded = id === "1";
			return {
				id,
				online: true,
				error: null,
				motd: modded ? "Create Aeronautics" : "A Minecraft Server",
				players: modded ? 3 : 12,
				max_players: modded ? 20 : 60,
				manifest: {
					minecraft: modded ? "1.21.1" : "1.21.11",
					loader: modded ? { type: "neoforge", version: "21.1.247" } : null,
					mods: Array.from({ length: modded ? 214 : 0 }, (_, i) => ({
						name: `mod-${i}.jar`, sha1: String(i), url: ""
					})),
					config: Array.from({ length: modded ? 410 : 0 }, (_, i) => ({
						path: `c${i}.toml`, sha1: String(i), url: "", force: false
					}))
				}
			};
		}
		case "mods": {
			const tab = String(args.tab);
			if (tab === "pack") {
				return Array.from({ length: 40 }, (_, i) => ({
					id: String(i), title: `packmod-${i}.jar`, description: "",
					downloads: 0, source: "pack", on_server: true, icon: null
				}));
			}
			if (tab === "installed") {
				return [
					{ id: "simplerpc.jar", title: "SimpleRPC-4.1.4.jar", description: "in deinem Profil",
					  downloads: 0, source: "profile", on_server: false, icon: null }
				];
			}
			const page = Number(args.offset ?? 0) / 40;
			const catalogue = [
				{ id: "sodium", title: "Sodium", description: "Moderne Rendering-Engine, deutlich mehr FPS",
				  downloads: 42_000_000, source: "modrinth", on_server: true,
				  icon: "https://cdn.modrinth.com/data/AANobbMI/icon.png" },
				{ id: "iris", title: "Iris Shaders", description: "Shader-Unterstützung, kompatibel mit Sodium",
				  downloads: 28_000_000, source: "modrinth", on_server: false,
				  icon: "https://cdn.modrinth.com/data/YL57xq9U/icon.png" },
				{ id: "cf:263420", title: "Xaero's Minimap", description: "Karte in der Ecke",
				  downloads: 241_000_000, source: "curseforge", on_server: true,
				  icon: "https://media.forgecdn.net/avatars/thumbnails/175/905/64/64/636426383151327212.png" },
				{ id: "chat-heads", title: "Chat Heads", description: "Zeigt den Kopf des Absenders im Chat",
				  downloads: 9_400_000, source: "modrinth", on_server: false,
				  icon: "https://cdn.modrinth.com/data/Wb5oqrBJ/icon.png" },
				{ id: "appleskin", title: "AppleSkin", description: "Zeigt Sättigung und Nährwerte an",
				  downloads: 31_000_000, source: "modrinth", on_server: false,
				  icon: "https://cdn.modrinth.com/data/EsAfCjCV/icon.png" },
				{ id: "jei", title: "Just Enough Items", description: "Rezepte nachschlagen",
				  downloads: 88_000_000, source: "modrinth", on_server: false,
				  icon: "https://cdn.modrinth.com/data/u6dRKJwZ/icon.png" }
			];
			return catalogue.map((hit, i) => ({ ...hit, id: page ? `${hit.id}-${page}-${i}` : hit.id,
				title: page ? `${hit.title} (Seite ${page + 1})` : hit.title }));
		}
		case "install_mods":
			return ["iris-neoforge.jar", "sodium-neoforge.jar"];
		case "remove_mods":
			return args.names;
		default:
			return null;
	}
}
