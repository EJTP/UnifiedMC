import type { Account, Job, Server } from "./types";

/**
 * Everything the window shows. The Rust side pushes into this; nothing here
 * fetches on its own, so a screen can never be half-populated by its own timing.
 */
class LauncherState {
	servers = $state<Server[]>([]);
	account = $state<Account | null>(null);
	job = $state<Job | null>(null);
	busyServer = $state<string | null>(null);

	get ready() {
		return this.servers.filter((s) => s.state === "ready").length;
	}
}

export const launcher = new LauncherState();

/** Stand-in until the Tauri commands land. Same shape the backend will send. */
export function loadFixtures() {
	launcher.account = { name: "EJTP", kind: "microsoft" };
	launcher.servers = [
		{
			id: "1",
			name: "All of Create Aeronautics",
			address: "194.54.88.19:25601",
			state: "needs-swap",
			minecraft: "1.21.1",
			loader: "NeoForge",
			mods: 214,
			config: 410,
			online: 3,
			max: 20,
			motd: "Create Aeronautics"
		},
		{
			id: "2",
			name: "Vanilla",
			address: "mc.example.com",
			state: "ready",
			minecraft: "1.21.11",
			loader: "",
			mods: 0,
			config: 0,
			online: 12,
			max: 60
		},
		{ id: "3", name: "Testserver", address: "10.0.0.5:25565", state: "offline" },
		{ id: "4", name: "Wird geprüft", address: "mc.pending.net", state: "checking" }
	];
}
