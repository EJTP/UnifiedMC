package dev.unifiedmc.hub;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.attribute.PosixFilePermissions;
import java.util.Set;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.minecraft.client.Minecraft;
import net.minecraft.client.User;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.ServerList;
import dev.unifiedmc.hub.mixin.ServerListAccessor;
import net.minecraft.network.chat.Component;

/**
 * Talks to the shell that launched us, through three small files in ~/.unifiedmc.
 *
 *   servers.json   we write   every server in the player's list, so the shell can scout ahead
 *   direct.json    we read    the ones this instance can already serve - join, do not relaunch
 *   handoff.json   we write   "the player wants that server", then we quit
 *
 * The mod decides nothing. The shell already knows how to ping a server and compare it against
 * an instance; the same rule written twice is the same rule wrong twice. Files instead of a
 * socket because the shell already waits on process exit - no port to pick, no handshake to
 * version, nothing to clean up when the game crashes.
 */
public final class Handoff {
	private static final Path DIR = Path.of(System.getProperty("user.home"), ".unifiedmc");
	private static final Path HANDOFF = DIR.resolve("handoff.json");
	private static final Path DIRECT = DIR.resolve("direct.json");
	private static final Path SERVERS = DIR.resolve("servers.json");
	private static final Path SESSION = DIR.resolve("session.json");

	/**
	 * The server this instance was provisioned for, set by the shell. Empty in the hub.
	 *
	 * The guard that stops an instance from bouncing its own join straight back at the shell
	 * and relaunching itself forever.
	 */
	private static final String READY = System.getProperty("unifiedmc.ready", "");

	/**
	 * Whether the UnifiedMC shell launched us and is watching the handoff files.
	 *
	 * Under someone else's launcher nobody reads the request and nobody closes this window, so
	 * handing off would just hang on a loading screen. Unmanaged we stay out of the way entirely
	 * and let vanilla connect - it shows its own version-mismatch message, which is the honest
	 * answer when there is no shell to fix the mismatch.
	 */
	private static final boolean MANAGED = Boolean.getBoolean("unifiedmc.managed");

	private Handoff() {
	}

	/** @return true if the connection was handed off and must be cancelled. */
	public static boolean request(String host, int port) {
		String target = host + ":" + port;
		if (target.equalsIgnoreCase(READY) || isDirect(target)) {
			return false;   // this instance can serve it - join for real, no relaunch
		}
		if (!MANAGED) {
			UnifiedMcHub.LOG.info("no shell watching, letting vanilla connect to {}", target);
			return false;
		}

		JsonObject request = new JsonObject();
		request.addProperty("host", host);
		request.addProperty("port", port);
		if (!write(HANDOFF, request.toString())) {
			// Let the join proceed. Worst case the player gets vanilla's own version-mismatch
			// screen, which beats a silently dead button.
			return false;
		}

		UnifiedMcHub.LOG.info("handing off to {}", target);

		// Stay on screen. The shell boots the next instance behind this window and only then
		// kills us, so the swap happens under a loading screen instead of across a bare desktop.
		// ponytail: no cancel button - if the shell dies the player has to close the window.
		// Give this screen a button the day the shell stops being the one that launched us.
		Minecraft.getInstance().setScreen(new HandoffScreen(target));
		return true;
	}

	/** Servers the shell has already cleared for this instance. Unknown means hand off. */
	private static boolean isDirect(String target) {
		try {
			if (!Files.exists(DIRECT)) {
				return false;
			}
			for (JsonElement entry : JsonParser.parseString(Files.readString(DIRECT)).getAsJsonArray()) {
				if (target.equalsIgnoreCase(entry.getAsString())) {
					UnifiedMcHub.LOG.info("{} needs nothing we do not have, connecting directly", target);
					return true;
				}
			}
		} catch (IOException | RuntimeException e) {
			UnifiedMcHub.LOG.warn("unreadable {}, handing off instead", DIRECT, e);
		}
		return false;
	}

	/**
	 * Hand the shell the session this process was launched with.
	 *
	 * Whoever started us - CurseForge, Prism, any launcher - already did the Microsoft dance, and
	 * the token is right here in memory. Passing it on means the shell can launch further
	 * instances as the same player without registering an Azure app of its own.
	 *
	 * It is a bearer token: owner-readable only, and it expires, at which point the player has to
	 * start from the launcher again.
	 */
	public static void publishSession(Minecraft client) {
		User user = client.getUser();
		String token = user.getAccessToken();
		if (token == null || token.isEmpty() || "0".equals(token)) {
			UnifiedMcHub.LOG.info("offline session, nothing to pass on");
			return;
		}

		JsonObject session = new JsonObject();
		session.addProperty("name", user.getName());
		session.addProperty("uuid", user.getProfileId().toString());
		session.addProperty("token", token);
		if (writeSecret(SESSION, session.toString())) {
			UnifiedMcHub.LOG.info("passed the {} session to the shell", user.getName());
		}
	}

	/** Hand the shell the player's server list so it can scout while they are still in the menu. */
	public static void publishServerList(Minecraft client) {
		ServerList list = new ServerList(client);
		list.load();

		JsonArray out = new JsonArray();
		for (int i = 0; i < list.size(); i++) {
			out.add(list.get(i).ip);
		}
		for (ServerData hidden : ((ServerListAccessor) list).unifiedmc$hidden()) {
			out.add(hidden.ip);   // Direct Connect history - the most likely next join of all
		}
		write(SERVERS, out.toString());
	}

	/** Same as {@link #write}, but the file is never readable by anyone but its owner. */
	private static boolean writeSecret(Path file, String content) {
		if (!write(file, content)) {
			return false;
		}
		try {
			Files.setPosixFilePermissions(file, PosixFilePermissions.fromString("rw-------"));
		} catch (IOException | UnsupportedOperationException e) {
			// Windows has no POSIX bits; the file still sits in the user's own profile directory.
			UnifiedMcHub.LOG.debug("cannot restrict {}", file, e);
		}
		return true;
	}

	/** Write then rename: the shell polls these files while we run and must never see half of one. */
	private static boolean write(Path file, String content) {
		Path tmp = file.resolveSibling(file.getFileName() + ".tmp");
		try {
			Files.createDirectories(file.getParent());
			Files.writeString(tmp, content);
			Files.move(tmp, file, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
			return true;
		} catch (IOException e) {
			UnifiedMcHub.LOG.error("cannot write {}", file, e);
			return false;
		}
	}
}
