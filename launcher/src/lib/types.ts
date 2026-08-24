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

export interface Manifest {
	minecraft: string;
	loader: Loader | null;
	mods: { name: string; sha1: string; url: string }[];
	config: { path: string; sha1: string; url: string; force: boolean }[];
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
	manifest_port: number;
	keep_open: boolean;
	curseforge_key: string;
	jvm_profile: string;
	jvm_args: string;
}

export interface Session {
	name: string;
	uuid: string;
	token: string;
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
