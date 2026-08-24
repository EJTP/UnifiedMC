package dev.unifiedmc.server;

import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.Set;
import java.util.stream.Stream;

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;
import net.neoforged.fml.common.Mod;

/**
 * Serves this server's own mods to UnifiedMC clients.
 *
 * The server is both the source of truth and the CDN: no Modrinth, no CurseForge, no guessing
 * which project a jar came from. Whatever sits in mods/ is what the client gets.
 *
 * Deliberately touches no Minecraft class - only the {@code @Mod} annotation and the JDK. That is
 * what makes it portable: a client mod is welded to one Minecraft version through mappings and
 * mixins, this one compiles against a single jar and moves to Fabric or a new version almost
 * unchanged.
 */
@Mod("unifiedmc")
public class UnifiedMcServer {
	private static final String CONFIG = "config/unifiedmc.properties";
	private static final String OUR_CONFIG = "unifiedmc.properties";
	private static final int BACKLOG = 16;

	/** Loaded by this server and also sent to clients: the shared pack. */
	private static final Path MODS = Path.of("mods");

	/**
	 * Sent to clients and never loaded here.
	 *
	 * A client mod in mods/ is how a NeoForge server dies on startup, so shaders, minimaps and
	 * anything else client-only lives in its own directory that the loader does not scan. Drop a
	 * jar in, restart, done - that is the whole "define which client mods are needed" mechanism.
	 */
	private static final Path CLIENT_MODS = Path.of("unifiedmc/client");

	/**
	 * Config that overrides what config/ says, for files where the two sides legitimately differ.
	 *
	 * A client mod never ran on this server, so its config here is whatever default got written -
	 * FancyMenu's, for instance, has no TitleScreen entry, because nothing ever customised one.
	 * Same names, different content, and shipping the server's copy gives every player the
	 * default menu.
	 */
	private static final Path CLIENT_CONFIG = Path.of("unifiedmc/client-config");

	/** Modpacks that need matching configs break without this; the client gets them too. */
	private static final Path CONFIG_DIR = Path.of("config");

	/**
	 * One name per line: things clients must NOT get. Matches a jar filename in mods/ or a path
	 * under config/.
	 *
	 * config/ becomes readable by anyone who can reach this port, so a webhook url or an api token
	 * in there belongs on this list.
	 */
	private static final Path SERVER_ONLY = Path.of("unifiedmc/server-only.txt");

	/** Backups and per-world state are not configuration; nobody else has any use for them. */
	private static final String[] NOT_CONFIG = {".bak", ".old", ".tmp", ".dat_old", ".log"};

	/** Directories under config/ holding one player's own state rather than the pack's settings. */
	private static final String[] PRIVATE_DIRS = {"jei/world/", "ftbchunks/", "ftbteams/"};

	/** Never ship ourselves - we are the publisher, the client has no use for us. */
	private static final String SELF = "unifiedmc-server";

	/** Short enough to guess is worse than none, because it looks like security. */
	private static final int MIN_TOKEN_LENGTH = 32;

	/** A jar nobody should be able to make us hold in memory. */
	private static final int MAX_UPLOAD_BYTES = 64 * 1024 * 1024;

	/** Slow enough that guessing is pointless, short enough not to be a denial of service. */
	private static final long FAILED_ATTEMPT_DELAY_MS = 1000;

	private byte[] adminToken;
	/** Kept so a rescan can rebuild the manifest without re-reading the file. */
	private Properties config;
	private final java.util.concurrent.atomic.AtomicLong lastFailure =
			new java.util.concurrent.atomic.AtomicLong();

	/** hash -> file, for serving. Holds mods AND configs, so it must never be the manifest source. */
	private final Map<String, Path> byHash = new HashMap<>();
	/** hash -> filename, the mods and only the mods */
	private final Map<String, String> modFiles = new LinkedHashMap<>();
	/** relative path under config/ -> hash, so the client knows where each file goes */
	private final Map<String, String> configFiles = new LinkedHashMap<>();
	/** those of them that came from client-config/ and must win over whatever is on the client */
	private final Set<String> forcedConfig = new HashSet<>();
	private volatile String manifest = "{}";

	public UnifiedMcServer() {
		try {
			start(load());
		} catch (IOException | RuntimeException e) {
			// A broken publisher must never take the game server down with it. Players can still
			// play; they just have to install mods the old way until this is fixed.
			System.err.println("[unifiedmc] not publishing: " + e);
			e.printStackTrace();
		}
	}

	private Properties load() throws IOException {
		Properties config = new Properties();
		Path file = Path.of(CONFIG);
		if (Files.exists(file)) {
			try (var in = Files.newInputStream(file)) {
				config.load(in);
			}
		}

		config.putIfAbsent("port", "25566");
		config.putIfAbsent("loader", "neoforge");

		// Not getProperty(key, detect...): Java evaluates that argument whether the key is
		// present or not, so a configured version would still go looking on disk - and fail
		// anywhere the loader is not installed the way we expect.
		String loaderVersion = config.getProperty("loader-version");
		if (loaderVersion == null || loaderVersion.isBlank()) {
			loaderVersion = detectNeoforgeVersion();
			config.setProperty("loader-version", loaderVersion);
		}
		config.putIfAbsent("minecraft", minecraftFor(loaderVersion));

		if (!Files.exists(file)) {   // leave the admin something to edit
			Files.createDirectories(file.getParent());
			try (var out = Files.newOutputStream(file)) {
				config.store(out, "UnifiedMC - what this server tells clients to install");
			}
		}
		return config;
	}

	/** A NeoForge server has exactly one of these on disk, and it is the version it is running. */
	private static String detectNeoforgeVersion() throws IOException {
		Path dir = Path.of("libraries/net/neoforged/neoforge");
		try (DirectoryStream<Path> found = Files.newDirectoryStream(dir)) {
			for (Path candidate : found) {
				return candidate.getFileName().toString();
			}
		}
		throw new IOException("cannot find a neoforge version under " + dir
				+ " - set loader-version in " + CONFIG);
	}

	/** NeoForge numbers itself after the Minecraft it targets: 21.1.247 is 1.21.1, 21.0.x is 1.21. */
	static String minecraftFor(String neoforgeVersion) {
		String[] part = neoforgeVersion.split("\\.");
		if (part.length < 2) {
			throw new IllegalArgumentException("odd neoforge version: " + neoforgeVersion);
		}
		return "0".equals(part[1]) ? "1." + part[0] : "1." + part[0] + "." + part[1];
	}

	private void start(Properties config) throws IOException {
		prepareLayout();
		publish(config);
		serve(config);
	}

	/** Read what is on disk and turn it into the manifest. Called again by /admin/rescan. */
	private void publish(Properties config) throws IOException {
		this.config = config;
		scan();

		StringBuilder mods = new StringBuilder();
		for (Map.Entry<String, String> mod : modFiles.entrySet()) {
			if (mods.length() > 0) {
				mods.append(',');
			}
			// relative: the client resolves it against whatever address it reached us on, so this
			// works behind any hostname, port forward or proxy without being told about it
			mods.append(String.format("{\"name\":%s,\"sha1\":\"%s\",\"url\":\"/mods/%s\"}",
					quote(mod.getValue()), mod.getKey(), mod.getKey()));
		}
		StringBuilder configs = new StringBuilder();
		for (Map.Entry<String, String> file : configFiles.entrySet()) {
			if (configs.length() > 0) {
				configs.append(',');
			}
			configs.append(String.format(
					"{\"path\":%s,\"sha1\":\"%s\",\"url\":\"/config/%s\",\"force\":%s}",
					quote(file.getKey()), file.getValue(), file.getValue(),
					forcedConfig.contains(file.getKey())));
		}
		manifest = String.format(
				"{\"minecraft\":%s,\"loader\":{\"type\":%s,\"version\":%s},"
						+ "\"mods\":[%s],\"config\":[%s]}",
				quote(config.getProperty("minecraft")), quote(config.getProperty("loader")),
				quote(config.getProperty("loader-version")), mods, configs);
	}

	private void serve(Properties config) throws IOException {
		int port = Integer.parseInt(config.getProperty("port"));
		HttpServer http = HttpServer.create(new InetSocketAddress(port), BACKLOG);
		http.createContext("/unifiedmc.json", this::serveManifest);
		http.createContext("/mods/", this::serveMod);
		http.createContext("/config/", this::serveMod);   // same store, addressed the same way

		// Absent unless a token is configured. Not a default token, not a warning - absent.
		// A write endpoint that is on by default is on for everyone who never read about it.
		String token = config.getProperty("admin-token", "").trim();
		if (token.length() >= MIN_TOKEN_LENGTH) {
			this.adminToken = token.getBytes(StandardCharsets.UTF_8);
			http.createContext("/admin/upload", this::adminUpload);
			http.createContext("/admin/delete", this::adminDelete);
			http.createContext("/admin/rescan", this::adminRescan);
			System.out.println("[unifiedmc] remote control enabled");
		} else if (!token.isEmpty()) {
			System.err.println("[unifiedmc] admin-token is shorter than " + MIN_TOKEN_LENGTH
					+ " characters; remote control stays off");
		}
		http.setExecutor(null);
		http.start();
		System.out.println("[unifiedmc] publishing " + byHash.size() + " mods on port " + port);
	}

	private void scan() throws IOException {
		Set<String> skip = serverOnly();
		int shared = collect(MODS, skip);
		int clientOnly = collect(CLIENT_MODS, skip);
		int configs = collectConfig(skip);
		System.out.println("[unifiedmc] " + shared + " shared, " + clientOnly + " client-only, "
				+ configs + " config files, " + skip.size() + " held back");
		if (modFiles.size() != shared + clientOnly) {
			throw new IOException("mod list is " + modFiles.size() + " but only " + (shared + clientOnly)
					+ " jars were collected - configs must never leak into it");
		}
	}

	private int collect(Path dir, Set<String> skip) throws IOException {
		if (!Files.isDirectory(dir)) {
			return 0;
		}
		int taken = 0;
		try (DirectoryStream<Path> found = Files.newDirectoryStream(dir, "*.jar")) {
			for (Path jar : found) {
				String name = jar.getFileName().toString();
				if (name.startsWith(SELF) || skip.contains(name)) {
					continue;
				}
				String hash = sha1(jar);
				byHash.put(hash, jar);
				modFiles.put(hash, name);
				taken++;
			}
		}
		return taken;
	}

	private int collectConfig(Set<String> skip) throws IOException {
		gatherConfig(CONFIG_DIR, skip);
		gatherConfig(CLIENT_CONFIG, skip);   // same relative path wins, so this overrides
		forcedConfig.addAll(clientConfigPaths());
		return configFiles.size();
	}

	private Set<String> clientConfigPaths() throws IOException {
		Set<String> paths = new HashSet<>();
		if (!Files.isDirectory(CLIENT_CONFIG)) {
			return paths;
		}
		try (Stream<Path> walk = Files.walk(CLIENT_CONFIG)) {
			walk.filter(Files::isRegularFile).forEach(file ->
					paths.add(CLIENT_CONFIG.relativize(file).toString().replace('\\', '/')));
		}
		return paths;
	}

	private void gatherConfig(Path root, Set<String> skip) throws IOException {
		if (!Files.isDirectory(root)) {
			return;
		}
		try (Stream<Path> walk = Files.walk(root)) {
			for (Path file : (Iterable<Path>) walk.filter(Files::isRegularFile)::iterator) {
				String relative = root.relativize(file).toString().replace('\\', '/');
				if (relative.equals(OUR_CONFIG) || skip.contains(relative)
						|| skip.contains(file.getFileName().toString())
						|| isPrivate(relative)) {
					continue;
				}
				String hash = sha1(file);
				byHash.put(hash, file);
				configFiles.put(relative, hash);
			}
		}
	}

	static boolean isPrivate(String relative) {
		String lower = relative.toLowerCase(java.util.Locale.ROOT);
		for (String suffix : NOT_CONFIG) {
			if (lower.endsWith(suffix)) {
				return true;
			}
		}
		for (String dir : PRIVATE_DIRS) {
			if (lower.startsWith(dir)) {
				return true;
			}
		}
		return false;
	}

	/** Mods this server needs but a client must never see - they would only crash it or bloat it. */
	private static Set<String> serverOnly() throws IOException {
		Set<String> held = new HashSet<>();
		if (!Files.isRegularFile(SERVER_ONLY)) {
			return held;
		}
		for (String line : Files.readAllLines(SERVER_ONLY)) {
			String name = line.trim();
			if (!name.isEmpty() && !name.startsWith("#")) {
				held.add(name);
			}
		}
		return held;
	}

	/** Leave the admin the directories, so the layout explains itself without documentation. */
	private static void prepareLayout() throws IOException {
		Files.createDirectories(CLIENT_MODS);
		Files.createDirectories(CLIENT_CONFIG);
		if (!Files.exists(SERVER_ONLY)) {
			Files.write(SERVER_ONLY, List.of(
					"# One jar filename per line: mods this server loads but clients must NOT get.",
					"# Client-only mods do not belong here - put those in unifiedmc/client/."));
		}
	}

	private static String sha1(Path file) throws IOException {
		try {
			MessageDigest digest = MessageDigest.getInstance("SHA-1");
			byte[] buffer = new byte[1 << 20];
			try (var in = Files.newInputStream(file)) {
				for (int read; (read = in.read(buffer)) > 0; ) {
					digest.update(buffer, 0, read);
				}
			}
			StringBuilder hex = new StringBuilder();
			for (byte b : digest.digest()) {
				hex.append(String.format("%02x", b));
			}
			return hex.toString();
		} catch (java.security.NoSuchAlgorithmException e) {
			throw new IOException("no SHA-1 in this JVM", e);   // cannot happen
		}
	}

	private void serveManifest(HttpExchange exchange) throws IOException {
		byte[] body = manifest.getBytes(StandardCharsets.UTF_8);
		exchange.getResponseHeaders().set("Content-Type", "application/json");
		exchange.sendResponseHeaders(200, body.length);
		try (OutputStream out = exchange.getResponseBody()) {
			out.write(body);
		}
	}

	private void serveMod(HttpExchange exchange) throws IOException {
		// looked up by hash, never by path: nothing a caller sends is ever used as a filename
		String path = exchange.getRequestURI().getPath();
		String hash = path.substring(path.lastIndexOf('/') + 1);
		Path jar = byHash.get(hash);
		if (jar == null || !Files.isRegularFile(jar)) {
			exchange.sendResponseHeaders(404, -1);
			exchange.close();
			return;
		}
		exchange.getResponseHeaders().set("Content-Type", "application/java-archive");
		exchange.sendResponseHeaders(200, Files.size(jar));
		try (OutputStream out = exchange.getResponseBody()) {
			Files.copy(jar, out);
		}
	}

	/**
	 * Is this request allowed to change anything?
	 *
	 * Constant-time comparison: String.equals returns as soon as two bytes differ, which leaks
	 * the length and every correct prefix to anyone willing to measure.
	 */
	private boolean authorised(HttpExchange exchange) throws IOException {
		String header = exchange.getRequestHeaders().getFirst("Authorization");
		String presented = header != null && header.startsWith("Bearer ")
				? header.substring("Bearer ".length())
				: "";

		if (adminToken != null
				&& MessageDigest.isEqual(adminToken, presented.getBytes(StandardCharsets.UTF_8))) {
			return true;
		}

		// visible in the log rather than silent, and slow enough that scanning is pointless
		System.err.println("[unifiedmc] rejected admin request from "
				+ exchange.getRemoteAddress().getAddress().getHostAddress());
		long wait = FAILED_ATTEMPT_DELAY_MS - (System.currentTimeMillis() - lastFailure.get());
		if (wait > 0) {
			try {
				Thread.sleep(wait);
			} catch (InterruptedException e) {
				Thread.currentThread().interrupt();
			}
		}
		lastFailure.set(System.currentTimeMillis());
		exchange.sendResponseHeaders(401, -1);
		exchange.close();
		return false;
	}

	/**
	 * A name that addresses a file, never one that builds a path.
	 *
	 * The caller is on the network. Anything with a separator, a parent reference or a drive
	 * letter is refused before it can reach the filesystem at all.
	 */
	static boolean isPlainFileName(String name) {
		return name != null
				&& !name.isEmpty()
				&& name.length() <= 200
				&& name.indexOf('/') < 0
				&& name.indexOf('\\') < 0
				&& name.indexOf('\0') < 0
				&& name.indexOf(':') < 0
				&& !name.startsWith(".")
				&& name.endsWith(".jar");
	}

	/** Which directory a request may write to. Never mods/ - that is what a restart is for. */
	private static Path adminTarget(String area, String name) {
		Path root = switch (area) {
			case "client" -> CLIENT_MODS;
			case "shared" -> Path.of("unifiedmc/staged");
			default -> null;
		};
		return root == null ? null : root.resolve(name);
	}

	private void adminUpload(HttpExchange exchange) throws IOException {
		if (!authorised(exchange)) {
			return;
		}
		Map<String, String> query = parseQuery(exchange.getRequestURI().getRawQuery());
		String name = query.get("name");
		String expected = query.get("sha1");
		Path target = adminTarget(query.getOrDefault("area", "client"), name);

		if (!isPlainFileName(name) || target == null || expected == null) {
			respond(exchange, 400, "name must be a plain .jar filename, sha1 is required");
			return;
		}

		byte[] body;
		try (var in = exchange.getRequestBody()) {
			body = in.readNBytes(MAX_UPLOAD_BYTES + 1);
		}
		if (body.length > MAX_UPLOAD_BYTES) {
			respond(exchange, 413, "larger than " + (MAX_UPLOAD_BYTES / 1024 / 1024) + " MB");
			return;
		}

        // Not about trust: a truncated upload would leave a corrupt jar the server then loads.
		String got = sha1(body);
		if (!MessageDigest.isEqual(got.getBytes(StandardCharsets.UTF_8),
				expected.getBytes(StandardCharsets.UTF_8))) {
			respond(exchange, 422, "hash mismatch: got " + got);
			return;
		}

		Files.createDirectories(target.getParent());
		Path temporary = target.resolveSibling(name + ".part");
		Files.write(temporary, body);
		Files.move(temporary, target, java.nio.file.StandardCopyOption.REPLACE_EXISTING);
		System.out.println("[unifiedmc] stored " + target);
		respond(exchange, 200, "stored " + target);
	}

	private void adminDelete(HttpExchange exchange) throws IOException {
		if (!authorised(exchange)) {
			return;
		}
		Map<String, String> query = parseQuery(exchange.getRequestURI().getRawQuery());
		String name = query.get("name");
		Path target = adminTarget(query.getOrDefault("area", "client"), name);

		if (!isPlainFileName(name) || target == null) {
			respond(exchange, 400, "name must be a plain .jar filename");
			return;
		}
		boolean gone = Files.deleteIfExists(target);
		System.out.println("[unifiedmc] " + (gone ? "removed " : "no such file ") + target);
		respond(exchange, gone ? 200 : 404, gone ? "removed " + name : "not here");
	}

	/** Re-read the directories and rebuild the manifest. Changes nothing on disk. */
	private void adminRescan(HttpExchange exchange) throws IOException {
		if (!authorised(exchange)) {
			return;
		}
		try {
			byHash.clear();
			modFiles.clear();
			configFiles.clear();
			forcedConfig.clear();
			publish(config);
			respond(exchange, 200, modFiles.size() + " mods, " + configFiles.size() + " config files");
		} catch (IOException | RuntimeException e) {
			respond(exchange, 500, "rescan failed: " + e);
		}
	}

	private static Map<String, String> parseQuery(String raw) {
		Map<String, String> values = new HashMap<>();
		if (raw == null) {
			return values;
		}
		for (String pair : raw.split("&")) {
			int split = pair.indexOf('=');
			if (split > 0) {
				values.put(
						java.net.URLDecoder.decode(pair.substring(0, split), StandardCharsets.UTF_8),
						java.net.URLDecoder.decode(pair.substring(split + 1), StandardCharsets.UTF_8));
			}
		}
		return values;
	}

	private static void respond(HttpExchange exchange, int status, String message)
			throws IOException {
		byte[] body = message.getBytes(StandardCharsets.UTF_8);
		exchange.getResponseHeaders().set("Content-Type", "text/plain; charset=utf-8");
		exchange.sendResponseHeaders(status, body.length);
		try (OutputStream out = exchange.getResponseBody()) {
			out.write(body);
		}
	}

	private static String sha1(byte[] data) throws IOException {
		try {
			StringBuilder hex = new StringBuilder();
			for (byte b : MessageDigest.getInstance("SHA-1").digest(data)) {
				hex.append(String.format("%02x", b));
			}
			return hex.toString();
		} catch (java.security.NoSuchAlgorithmException e) {
			throw new IOException("no SHA-1 in this JVM", e);
		}
	}

	private static String quote(String value) {
		return "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
	}

	/** Runnable without a server: {@code java -cp out dev.unifiedmc.server.UnifiedMcServer}. */
	public static void main(String[] args) {
		assert "1.21.1".equals(minecraftFor("21.1.247")) : minecraftFor("21.1.247");
		assert "1.21".equals(minecraftFor("21.0.167")) : minecraftFor("21.0.167");
		assert "1.20.4".equals(minecraftFor("20.4.100")) : minecraftFor("20.4.100");
		assert "\"a\\\"b\"".equals(quote("a\"b")) : quote("a\"b");
		assert isPrivate("create-common-1.toml.bak");
		assert isPrivate("jei/world/server/Minecraft Server/bookmarks.json");
		assert isPrivate("debug.LOG");
		assert !isPrivate("create-common.toml");
		assert !isPrivate("fancymenu/customization/main.txt");

		// a name addresses a file; it must never be able to build a path
		assert isPlainFileName("sodium.jar");
		assert !isPlainFileName("../../server.properties");
		assert !isPlainFileName("sub/dir.jar");
		assert !isPlainFileName("sub\\dir.jar");
		assert !isPlainFileName("C:evil.jar");
		assert !isPlainFileName(".hidden.jar");
		assert !isPlainFileName("notajar.txt");
		assert !isPlainFileName("");
		assert !isPlainFileName(null);
		System.out.println("self-check ok");
	}
}
