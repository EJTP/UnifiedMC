export type ServerState = "checking" | "offline" | "ready" | "needs-swap" | "unknown";

export interface Server {
	id: string;
	name: string;
	address: string;
	state: ServerState;
	minecraft?: string;
	loader?: string;
	mods?: number;
	config?: number;
	online?: number;
	max?: number;
	motd?: string;
}

export interface Job {
	phase: string;
	detail: string;
	done: number;
	total: number;
}

export interface Account {
	name: string;
	kind: "microsoft" | "offline";
}
