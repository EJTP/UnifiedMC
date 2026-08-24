export interface SavedServer {
	id: string;
	name: string;
	address: string;
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
	motd: string;
	players: number;
	max_players: number;
	manifest: Manifest | null;
}

export interface Settings {
	memory: number;
	offline_name: string;
	manifest_port: number;
	keep_open: boolean;
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
	icon: string | null;
}
