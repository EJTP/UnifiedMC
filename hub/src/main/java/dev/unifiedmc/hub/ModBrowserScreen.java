package dev.unifiedmc.hub;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import io.wispforest.owo.ui.base.BaseOwoScreen;
import io.wispforest.owo.ui.component.LabelComponent;
import io.wispforest.owo.ui.component.UIComponents;
import io.wispforest.owo.ui.container.FlowLayout;
import io.wispforest.owo.ui.container.UIContainers;
import io.wispforest.owo.ui.core.Color;
import io.wispforest.owo.ui.core.HorizontalAlignment;
import io.wispforest.owo.ui.core.Insets;
import io.wispforest.owo.ui.core.OwoUIAdapter;
import io.wispforest.owo.ui.core.Sizing;
import io.wispforest.owo.ui.core.Surface;
import io.wispforest.owo.ui.core.VerticalAlignment;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;
import org.jetbrains.annotations.NotNull;

/**
 * Your own mods for one server, on top of what that server already ships.
 *
 * The searching happens in the shell - it talks to both catalogues and, crucially, knows every jar
 * the server sends, so anything the player would end up with twice never appears here. This screen
 * asks and draws; it decides nothing.
 */
public class ModBrowserScreen extends BaseOwoScreen<FlowLayout> {
	private static final Path DIR = Path.of(System.getProperty("user.home"), ".unifiedmc");
	private static final Path QUERY = DIR.resolve("query.json");
	private static final Path CATALOG = DIR.resolve("catalog.json");
	private static final long POLL_MS = 200;
	private static final int ICON = 26;

	/** Which list the player is looking at. The shell answers all three the same way. */
	public enum Tab {
		SEARCH("search", "Katalog"),
		INSTALLED("installed", "Installiert"),
		PACK("pack", "Im Pack");

		final String mode;
		final String label;

		Tab(String mode, String label) {
			this.mode = mode;
			this.label = label;
		}
	}

	private final Screen parent;
	private final String server;
	private final Tab tab;
	private final Set<String> picked = new LinkedHashSet<>();
	private final List<JsonObject> hits = new ArrayList<>();

	private FlowLayout list;
	private LabelComponent note;
	private long pending = -1;
	private long lastPoll;
	private boolean busy;

	public ModBrowserScreen(Screen parent, String server) {
		this(parent, server, Tab.SEARCH);
	}

	public ModBrowserScreen(Screen parent, String server, Tab tab) {
		this.parent = parent;
		this.server = server;
		this.tab = tab;
	}

	@Override
	protected @NotNull OwoUIAdapter<FlowLayout> createAdapter() {
		return OwoUIAdapter.create(this, UIContainers::verticalFlow);
	}

	@Override
	protected void build(FlowLayout root) {
		root.surface(Surface.blur(4f, 4f).and(Surface.flat(Ui.SCRIM_BOTTOM)));
		root.horizontalAlignment(HorizontalAlignment.CENTER);
		root.verticalAlignment(VerticalAlignment.CENTER);
		root.padding(Insets.of(14));

		FlowLayout card = UIContainers.verticalFlow(Sizing.fill(78), Sizing.fill(90));
		card.gap(8);
		card.surface(Surface.flat(Ui.CARD_TOP).and(Surface.outline(Ui.EDGE)));
		card.padding(Insets.of(14));
		card.horizontalAlignment(HorizontalAlignment.CENTER);

		card.child(UIComponents.label(Component.literal("Eigene Mods")).color(Color.ofArgb(Ui.TEXT)));
		card.child(UIComponents.label(Component.literal(
						server + "  ·  nur was ohne Server-Seite laeuft"))
				.color(Color.ofArgb(Ui.TEXT_FAINT)));
		card.child(UIComponents.label(Component.literal(
						"grau = kommt schon mit dem Pack, nichts zu tun"))
				.color(Color.ofArgb(Ui.TEXT_FAINT)));

		var search = UIComponents.textBox(Sizing.fill(100));
		search.onChanged().subscribe(text -> ask(text, null));
		card.child(search);

		list = UIContainers.verticalFlow(Sizing.fill(100), Sizing.content());
		list.gap(3);
		var scroller = UIContainers.verticalScroll(Sizing.fill(100), Sizing.expand(), list);
		scroller.surface(Surface.flat(0x2A000000));
		scroller.padding(Insets.of(6));
		card.child(scroller);

		FlowLayout bar = UIContainers.horizontalFlow(Sizing.fill(100), Sizing.content());
		bar.gap(6);
		bar.verticalAlignment(VerticalAlignment.CENTER);
		note = UIComponents.label(Component.literal("Suche ...")).color(Color.ofArgb(Ui.TEXT_DIM));
		bar.child(note);
		bar.child(UIComponents.box(Sizing.expand(), Sizing.fixed(1)).color(Color.ofArgb(0)));
		if (tab != Tab.PACK) {
			bar.child(UIComponents.button(
							Component.literal(tab == Tab.INSTALLED ? "Entfernen" : "Installieren"),
							pressed -> apply())
					.sizing(Sizing.fixed(84), Sizing.fixed(18)));
		}
		bar.child(UIComponents.button(Component.literal("Zurueck"),
						pressed -> this.minecraft.setScreen(parent))
				.sizing(Sizing.fixed(66), Sizing.fixed(18)));
		card.child(bar);

		root.child(card);
		ask("", null);
	}

	private String hint() {
		return switch (tab) {
			case SEARCH -> "nur was ohne Server-Seite laeuft  ·  grau = kommt schon mit dem Pack";
			case INSTALLED -> "deine eigenen Mods fuer diesen Server";
			case PACK -> "alles was der Server ausliefert";
		};
	}

	/** One request at a time; the id is how we recognise the answer that belongs to it. */
	private void ask(String query, List<String> change) {
		pending = System.nanoTime();
		busy = true;
		say(change == null ? "Laedt ..."
				: tab == Tab.INSTALLED ? "Wird entfernt ..." : "Wird installiert ...", Ui.ACCENT);

		JsonObject request = new JsonObject();
		request.addProperty("id", Long.toString(pending));
		request.addProperty("server", server);
		request.addProperty("query", query);
		request.addProperty("mode", tab.mode);
		if (change != null) {
			JsonArray wanted = new JsonArray();
			change.forEach(wanted::add);
			request.add(tab == Tab.INSTALLED ? "remove" : "install", wanted);
		}

		try {
			Files.createDirectories(DIR);
			Path tmp = QUERY.resolveSibling("query.json.tmp");
			Files.writeString(tmp, request.toString());
			Files.move(tmp, QUERY, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
		} catch (IOException e) {
			busy = false;
			say("Shell nicht erreichbar", Ui.BAD);
		}
	}

	private void apply() {
		if (picked.isEmpty() || busy) {
			return;
		}
		ask("", new ArrayList<>(picked));
	}

	private void say(String text, int colour) {
		if (note != null) {
			note.text(Component.literal(text));
			note.color(Color.ofArgb(colour));
		}
	}

	@Override
	public void tick() {
		super.tick();
		long now = System.currentTimeMillis();
		if (!busy || now - lastPoll < POLL_MS) {
			return;
		}
		lastPoll = now;

		try {
			if (!Files.exists(CATALOG)) {
				return;
			}
			JsonObject answer = JsonParser.parseString(Files.readString(CATALOG)).getAsJsonObject();
			if (!Long.toString(pending).equals(answer.get("id").getAsString())) {
				return;   // an older answer, still on its way out
			}
			busy = false;

			if (answer.has("installed") || answer.has("removed")) {
				boolean removal = answer.has("removed");
				int count = answer.getAsJsonArray(removal ? "removed" : "installed").size();
				picked.clear();
				say(count == 0 ? "Nichts geaendert"
						: count + (removal ? " entfernt" : " installiert, liegt in deinem Profil"),
						count == 0 ? Ui.WARN : Ui.GOOD);
				ask("", null);   // the list just changed under us
				return;
			}

			hits.clear();
			for (JsonElement hit : answer.getAsJsonArray("hits")) {
				hits.add(hit.getAsJsonObject());
			}
			say(hits.isEmpty() ? "Nichts gefunden fuer diese Version" : hits.size() + " Treffer",
					hits.isEmpty() ? Ui.WARN : Ui.TEXT_DIM);
			rebuild();
		} catch (IOException | RuntimeException e) {
			// keep waiting; a half-written answer is not an error
		}
	}

	private void rebuild() {
		if (list == null) {
			return;
		}
		list.clearChildren();
		for (JsonObject hit : hits) {
			list.child(row(hit));
		}
	}

	private FlowLayout row(JsonObject hit) {
		String id = hit.get("id").getAsString();
		boolean shipped = hit.has("on_server") && hit.get("on_server").getAsBoolean();
		boolean chosen = picked.contains(id);

		FlowLayout entry = UIContainers.horizontalFlow(Sizing.fill(100), Sizing.content());
		entry.gap(8);
		entry.surface(Surface.flat(shipped ? Ui.TRACK : chosen ? Ui.FILL_LEFT : Ui.ROW));
		entry.verticalAlignment(VerticalAlignment.CENTER);
		entry.padding(Insets.of(6));
		if (!shipped) {
			entry.mouseDown().subscribe((event, doubleClick) -> {
				if (!picked.remove(id)) {
					picked.add(id);
				}
				rebuild();
				return true;
			});
		}

		Icons.Loaded icon = hit.has("icon") ? Icons.get(hit.get("icon").getAsString()) : null;
		if (icon != null) {
			entry.child(UIComponents.texture(icon.id(), 0, 0, icon.width(), icon.height(),
							icon.width(), icon.height())
					.sizing(Sizing.fixed(ICON), Sizing.fixed(ICON)));
		} else {
			entry.child(UIComponents.box(Sizing.fixed(ICON), Sizing.fixed(ICON))
					.color(Color.ofArgb(Ui.TRACK)).fill(true));
		}

		FlowLayout text = UIContainers.verticalFlow(Sizing.expand(), Sizing.content());
		text.gap(2);
		text.child(UIComponents.label(Component.literal(hit.get("title").getAsString()))
				.color(Color.ofArgb(shipped ? Ui.TEXT_DIM : Ui.TEXT)));
		text.child(UIComponents.label(Component.literal(hit.get("description").getAsString()))
				.maxWidth(320).color(Color.ofArgb(Ui.TEXT_FAINT)));
		entry.child(text);

		if (shipped) {
			entry.child(UIComponents.label(Component.literal("im Pack"))
					.color(Color.ofArgb(Ui.GOOD)));
			return entry;
		}
		String source = hit.get("source").getAsString();
		entry.child(UIComponents.label(Component.literal(compact(hit.get("downloads").getAsLong())
						+ "  " + ("modrinth".equals(source) ? "MR" : "CF")))
				.color(Color.ofArgb(Ui.TEXT_FAINT)));
		return entry;
	}

	private static String compact(long count) {
		if (count >= 1_000_000) {
			return count / 1_000_000 + "M";
		}
		return count >= 1_000 ? count / 1_000 + "k" : Long.toString(count);
	}
}
