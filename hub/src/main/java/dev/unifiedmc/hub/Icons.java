package dev.unifiedmc.hub;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;

import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.Identifier;

/**
 * Catalogue artwork, uploaded once and kept.
 *
 * The shell has already downloaded and, where needed, converted these to PNG - the game only ever
 * sees a local file it can decode, so a mod with a webp logo cannot take the screen down.
 */
public final class Icons {
	private static final Path DIR = Path.of(System.getProperty("user.home"), ".unifiedmc", "icons");
	private static final Map<String, Loaded> CACHE = new HashMap<>();

	/** @param id where the texture lives, or null if it could not be read */
	public record Loaded(Identifier id, int width, int height) {
	}

	private Icons() {
	}

	public static Loaded get(String fileName) {
		if (fileName == null || fileName.isEmpty()) {
			return null;
		}
		if (CACHE.containsKey(fileName)) {
			return CACHE.get(fileName);
		}

		Loaded loaded = load(fileName);
		CACHE.put(fileName, loaded);   // cache failures too: retrying every frame helps nobody
		return loaded;
	}

	private static Loaded load(String fileName) {
		Path file = DIR.resolve(fileName);
		if (!file.getFileName().toString().equals(fileName) || !Files.isRegularFile(file)) {
			return null;   // the name comes from outside; it addresses a file, it does not build a path
		}
		try (InputStream in = Files.newInputStream(file)) {
			NativeImage image = NativeImage.read(in);
			Identifier id = Identifier.fromNamespaceAndPath("unifiedmc",
					"icon/" + fileName.replace(".png", ""));
			Minecraft.getInstance().getTextureManager()
					.register(id, new DynamicTexture(() -> "unifiedmc-" + fileName, image));
			return new Loaded(id, image.getWidth(), image.getHeight());
		} catch (IOException | RuntimeException e) {
			UnifiedMcHub.LOG.debug("cannot load icon {}", fileName, e);
			return null;
		}
	}
}
