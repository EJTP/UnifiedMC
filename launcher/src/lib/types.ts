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
	/** Which accent the window is painted in. See `theme.ts` for the names. */
	accent: string;
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
	/** Who wrote it. A title alone does not tell two mods of the same name apart. */
	author: string;
	/** The build that would be installed, so what arrives is not a surprise. */
	version: string;
	/** The project's own page, or empty for a jar already on this disk. */
	url: string;
	/** Whether it can be taken away again. What the server ships cannot. */
	removable: boolean;
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

/**
 * A server running on this machine. The record as it was written down, plus what it is doing
 * right now - the two arrive together because a row needs both to draw one line.
 */
export interface HostedServer {
	id: string;
	name: string;
	minecraft: string;
	loader: string | null;
	loader_version: string | null;
	port: number;
	memory: number;
	/** The pack it was built from, when it was built from one. */
	source: string | null;
	command: string[];
	eula: boolean;
	/** Whether the publisher mod is in place: whether a player with no mods can just join. */
	publishes: boolean;
	running: boolean;
	players: string[];
	/** What to type into Minecraft's own server list on this machine. */
	address: string;
	directory: string;
}

/** One line a hosted server printed, and which server printed it. */
export interface ConsoleLine {
	id: string;
	line: string;
}

/**
 * How much one server or instance has been played. Days are whole days since the epoch, so
 * a bucket becomes a date with one multiplication and needs no date library either side.
 */
export interface Played {
	seconds: number;
	/** Unix seconds of the last session's end. 0 for something never finished. */
	last: number;
	sessions: number;
	days: Record<number, number>;
}

/** A release on GitHub that is newer than this build. */
export interface Release {
	current: string;
	latest: string;
	url: string;
	notes: string;
	download: string | null;
}

/** What the player has to do while the sign-in waits for them. */
export interface SignInPrompt {
	user_code: string;
	verification_uri: string;
	expires_in: number;
}
