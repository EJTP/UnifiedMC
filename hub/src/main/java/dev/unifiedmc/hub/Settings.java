package dev.unifiedmc.hub;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.minecraft.client.Minecraft;
import net.minecraft.sounds.SoundSource;

/**
 * How the hub looks and sounds, kept in one small file of our own.
 *
 * Not Minecraft's options.txt: the hub rewrites that at startup, and mixing "what the player chose
 * for the hub" with "what Minecraft happens to have persisted" makes both unpredictable.
 */
public final class Settings {
	private static final Path FILE = Path.of(System.getProperty("user.home"), ".unifiedmc", "ui.json");

	/** 0 means Minecraft's own automatic scale. */
	public static final int SCALE_AUTO = 0;
	public static final int[] SCALES = {SCALE_AUTO, 1, 2, 3, 4};

	/** Megabytes. 0 lets the shell size it from how many mods the pack actually has. */
	public static final int RAM_AUTO = 0;
	public static final int[] RAM = {RAM_AUTO, 3072, 4096, 6144, 8192};

	private static int scale = 3;   // the hub is a launcher, read from a metre away; small is wrong
	private static boolean muted = true;
	private static int ram = RAM_AUTO;
	private static boolean loaded;

	private Settings() {
	}

	public static int scale() {
		load();
		return scale;
	}

	public static boolean muted() {
		load();
		return muted;
	}

	public static int ram() {
		load();
		return ram;
	}

	/** Read by the shell when it launches an instance, so it only takes effect on the next start. */
	public static void setRam(int megabytes) {
		load();
		ram = megabytes;
		save();
	}

	public static void set(int newScale, boolean newMuted) {
		load();
		scale = newScale;
		muted = newMuted;
		save();
		apply(Minecraft.getInstance());
	}

	private static synchronized void load() {
		if (loaded) {
			return;
		}
		loaded = true;
		try {
			if (Files.exists(FILE)) {
				JsonObject saved = JsonParser.parseString(Files.readString(FILE)).getAsJsonObject();
				scale = saved.has("scale") ? saved.get("scale").getAsInt() : scale;
				muted = !saved.has("muted") || saved.get("muted").getAsBoolean();
				ram = saved.has("ram") ? saved.get("ram").getAsInt() : RAM_AUTO;
			}
		} catch (IOException | RuntimeException e) {
			UnifiedMcHub.LOG.warn("cannot read {}, using defaults", FILE, e);
		}
	}

	private static void save() {
		JsonObject out = new JsonObject();
		out.addProperty("scale", scale);
		out.addProperty("muted", muted);
		out.addProperty("ram", ram);
		try {
			Files.createDirectories(FILE.getParent());
			Path tmp = FILE.resolveSibling("ui.json.tmp");
			Files.writeString(tmp, out.toString());
			Files.move(tmp, FILE, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
		} catch (IOException e) {
			UnifiedMcHub.LOG.error("cannot write {}", FILE, e);
		}
	}

	/** Push the choice into Minecraft and make it take effect now, not on the next launch. */
	public static void apply(Minecraft client) {
		load();
		if (client.options.guiScale().get() != scale) {
			client.options.guiScale().set(scale);
			client.resizeDisplay();
		}
		double volume = muted ? 0.0 : 1.0;
		if (client.options.getSoundSourceVolume(SoundSource.MASTER) != volume) {
			client.options.getSoundSourceOptionInstance(SoundSource.MASTER).set(volume);
			if (muted) {
				client.getSoundManager().stop();
			}
		}
		client.options.save();
	}
}
