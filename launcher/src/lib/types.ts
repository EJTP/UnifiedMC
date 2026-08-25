export interface SavedServer {
	id: string;
	name: string;
	address: string;
	/** What the player chose to run here, when the server itself announces no loader. */
	loader: string | null;
	/** Which Minecraft, when detection would pick the wrong one. */
	minecraft: string | null;
}

/** A setup that exists on its own: the player picks the version and what goes in it. */
export interface Instance {
	id: string;
	name: string;
	minecraft: string;
	loader: string | null;
	/** A specific build, or null for whatever is newest when it launches. */
	loader_version: string | null;
	source: string | null;
}

export interface Loader {
	type: string;
	version: string | null;
}

/** One file the server publishes: a mod, a datapack, a resource pack or a shader. */
export interface ModEntry {
	name: string;
	sha1: string;
	url: string;
}

/** The four categories the browser offers, spelled the way the commands expect them. */
export type Kind = "mod" | "resourcepack" | "shader" | "datapack";

export interface Manifest {
	minecraft: string;
	loader: Loader | null;
	mods: ModEntry[];
	config: { path: string; sha1: string; url: string; force: boolean }[];
	/** The world's own data, so recipes and loot on the client match what the server runs. */
	datapacks: ModEntry[];
	resourcepacks: ModEntry[];
	shaders: ModEntry[];
}

export interface ServerStatus {
	id: string;
	online: boolean;
	error: string | null;
	motd: MotdSpan[];
	icon: string | null;
	players: number;
	max_players: number;
	manifest: Manifest | null;
}

export interface Settings {
	/** "system" follows the machine; otherwise the language the player picked. */
	language: "system" | "de" | "en";
	memory: number;
	offline_name: string;
	keep_open: boolean;
	jvm_profile: string;
	jvm_args: string;
}

export interface Session {
	name: string;
	uuid: string;
	kind: "microsoft" | "offline";
}

export interface Progress {
	phase: string;
	detail: string;
	done: number;
	total: number;
}

export interface Hit {
	id: string;
	title: string;
	description: string;
	downloads: number;
	source: "modrinth" | "curseforge" | "pack" | "profile";
	on_server: boolean;
	installed: boolean;
	icon: string | null;
}

/** One run of the server description, already styled the way the game would draw it. */
export interface MotdSpan {
	text: string;
	color?: string;
	bold?: boolean;
	italic?: boolean;
	underlined?: boolean;
	strikethrough?: boolean;
	obfuscated?: boolean;
}

/** What the player has to do while the sign-in waits for them. */
export interface SignInPrompt {
	user_code: string;
	verification_uri: string;
	expires_in: number;
}
