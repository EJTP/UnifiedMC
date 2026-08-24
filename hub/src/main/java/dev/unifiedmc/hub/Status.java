package dev.unifiedmc.hub;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

/**
 * What the shell has found out about each server in the list.
 *
 * The hub cannot ping anything itself without blocking its own render thread, and the shell is
 * already doing it - so this is just a reader over the file the shell keeps current.
 */
public final class Status {
	private static final Path FILE =
			Path.of(System.getProperty("user.home"), ".unifiedmc", "status.json");
	private static final long POLL_MS = 300;

	private static String raw = "{}";
	private static JsonObject parsed = new JsonObject();
	private static long lastRead;

	private Status() {
	}

	/** The whole file as text - cheap to compare, so a screen can tell whether to rebuild. */
	public static synchronized String raw() {
		long now = System.currentTimeMillis();
		if (now - lastRead >= POLL_MS) {
			lastRead = now;
			try {
				if (Files.exists(FILE)) {
					String content = Files.readString(FILE);
					if (!content.equals(raw)) {
						parsed = JsonParser.parseString(content).getAsJsonObject();
						raw = content;
					}
				}
			} catch (IOException | RuntimeException e) {
				// a torn read leaves the previous state in place
			}
		}
		return raw;
	}

	/** The shell keys by "host:port"; the server list stores whatever the player typed. */
	public static synchronized JsonObject of(String ip) {
		raw();
		String key = ip.contains(":") ? ip : ip + ":25565";
		return parsed.has(key) && parsed.get(key).isJsonObject() ? parsed.getAsJsonObject(key) : null;
	}

	public static int dot(JsonObject state) {
		if (state == null) {
			return Ui.TEXT_FAINT;
		}
		if (!state.get("online").getAsBoolean()) {
			return Ui.BAD;
		}
		if (!state.has("minecraft")) {
			return Ui.WARN;
		}
		return state.get("ready").getAsBoolean() ? Ui.GOOD : Ui.WARN;
	}

	public static String line(JsonObject state) {
		if (state == null) {
			return "wird geprueft ...";
		}
		if (!state.get("online").getAsBoolean()) {
			return "nicht erreichbar";
		}
		if (!state.has("minecraft")) {
			return "erreichbar, meldet keine Mods";
		}

		StringBuilder line = new StringBuilder(state.get("minecraft").getAsString());
		String loader = state.get("loader").getAsString();
		if (!loader.isEmpty()) {
			line.append("  ").append(loader);
		}
		line.append("  ·  ").append(state.get("mods").getAsInt()).append(" Mods");
		if (state.get("config").getAsInt() > 0) {
			line.append(" + ").append(state.get("config").getAsInt()).append(" Configs");
		}
		line.append(state.get("ready").getAsBoolean() ? "  ·  bereit" : "  ·  Wechsel noetig");
		if (state.has("online_players")) {
			line.append("  ·  ").append(state.get("online_players").getAsInt())
					.append("/").append(state.get("max_players").getAsInt());
		}
		return line.toString();
	}
}
