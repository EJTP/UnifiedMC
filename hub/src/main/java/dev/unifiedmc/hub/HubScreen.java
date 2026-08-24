package dev.unifiedmc.hub;

import java.util.ArrayList;
import java.util.List;

import com.google.gson.JsonObject;
import io.wispforest.owo.ui.base.BaseOwoScreen;
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
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.ServerList;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import net.minecraft.network.chat.Component;
import org.jetbrains.annotations.NotNull;

/**
 * The only screen the player normally sees: which servers there are, what each one actually is,
 * and a way into each.
 *
 * Vanilla's list shows a name, a ping bar and a version string. Here the shell has already probed
 * every entry, so a row can say what the server really wants - loader, mod count, whether joining
 * costs a restart at all - before the player commits to it.
 *
 * Laid out with owo-ui rather than by hand: the sizes are proportions, so it holds together in a
 * small window instead of being pinned to pixel offsets that only fit one resolution.
 */
public class HubScreen extends BaseOwoScreen<FlowLayout> {
	private final List<ServerData> servers = new ArrayList<>();
	private FlowLayout list;
	private String rendered = "";

	@Override
	protected @NotNull OwoUIAdapter<FlowLayout> createAdapter() {
		return OwoUIAdapter.create(this, UIContainers::verticalFlow);
	}

	@Override
	protected void build(FlowLayout root) {
		root.surface(Surface.blur(4f, 4f).and(Surface.flat(Ui.SCRIM_BOTTOM)))
				.horizontalAlignment(HorizontalAlignment.CENTER)
				.verticalAlignment(VerticalAlignment.CENTER)
				.padding(Insets.of(14));

		FlowLayout card = UIContainers.verticalFlow(Sizing.fill(72), Sizing.fill(88));
		card.gap(8);
		card.surface(Surface.flat(Ui.CARD_TOP).and(Surface.outline(Ui.EDGE)));
		card.padding(Insets.of(14));
		card.horizontalAlignment(HorizontalAlignment.CENTER);

		card.child(UIComponents.label(Component.literal("U N I F I E D  M C"))
				.color(Color.ofArgb(Ui.TEXT_FAINT)));
		card.child(UIComponents.label(Component.literal("Server"))
				.color(Color.ofArgb(Ui.TEXT)).margins(Insets.bottom(4)));

		list = UIContainers.verticalFlow(Sizing.fill(100), Sizing.content());
		list.gap(4);
		var scroller = UIContainers.verticalScroll(Sizing.fill(100), Sizing.expand(), list);
		scroller.surface(Surface.flat(0x2A000000));
		scroller.padding(Insets.of(6));
		card.child(scroller);

		card.child(footer());
		root.child(card);

		reload();
	}

	private FlowLayout footer() {
		FlowLayout bar = UIContainers.horizontalFlow(Sizing.fill(100), Sizing.content());
		bar.gap(6);
		bar.verticalAlignment(VerticalAlignment.CENTER);
		bar.margins(Insets.top(8));

		var address = UIComponents.textBox(Sizing.expand());
		bar.child(address);
		bar.child(UIComponents.button(Component.literal("Hinzufuegen"), pressed -> {
			add(address.getValue().trim());
			address.setValue("");
		}).sizing(Sizing.fixed(78), Sizing.fixed(18)));
		bar.child(UIComponents.button(Component.literal("Einstellungen"),
						pressed -> this.minecraft.setScreen(new SettingsScreen(this)))
				.sizing(Sizing.fixed(92), Sizing.fixed(18)));
		bar.child(UIComponents.button(Component.literal("Beenden"), pressed -> this.minecraft.stop())
				.sizing(Sizing.fixed(62), Sizing.fixed(18)));
		return bar;
	}

	private void add(String typed) {
		if (typed.isEmpty()) {
			return;
		}
		ServerList saved = new ServerList(this.minecraft);
		saved.load();
		saved.add(new ServerData(typed, typed, ServerData.Type.OTHER), false);
		saved.save();
		reload();
	}

	private void reload() {
		servers.clear();
		ServerList saved = new ServerList(this.minecraft);
		saved.load();
		for (int i = 0; i < saved.size(); i++) {
			servers.add(saved.get(i));
		}
		servers.addAll(((dev.unifiedmc.hub.mixin.ServerListAccessor) saved).unifiedmc$hidden());
		Handoff.publishServerList(this.minecraft);
		rendered = "";
		redraw();
	}

	@Override
	public void tick() {
		super.tick();
		redraw();   // the shell probes in the background; rows fill in as answers land
	}

	/** Rebuild only when something actually changed - otherwise every tick throws the list away. */
	private void redraw() {
		if (list == null) {
			return;
		}
		String state = Status.raw();
		if (state.equals(rendered)) {
			return;
		}
		rendered = state;

		list.clearChildren();
		if (servers.isEmpty()) {
			list.child(UIComponents.label(Component.literal("Noch kein Server. Adresse unten eintragen."))
					.color(Color.ofArgb(Ui.TEXT_FAINT)).margins(Insets.of(10)));
			return;
		}
		for (ServerData server : servers) {
			list.child(row(server));
		}
	}

	private FlowLayout row(ServerData server) {
		JsonObject state = Status.of(server.ip);

		// content-sized, never fixed: three stacked lines plus padding do not fit a magic number,
		// and what does not fit gets clipped
		FlowLayout entry = UIContainers.horizontalFlow(Sizing.fill(100), Sizing.content());
		entry.gap(8);
		entry.surface(Surface.flat(Ui.ROW));
		entry.verticalAlignment(VerticalAlignment.CENTER);
		entry.padding(Insets.of(8));
		entry.mouseDown().subscribe((event, doubleClick) -> {
			join(server);
			return true;
		});

		entry.child(UIComponents.box(Sizing.fixed(4), Sizing.fixed(28))
				.color(Color.ofArgb(Status.dot(state))).fill(true));

		FlowLayout text = UIContainers.verticalFlow(Sizing.expand(), Sizing.content());
		text.gap(2);
		text.child(UIComponents.label(Component.literal(server.name)).color(Color.ofArgb(Ui.TEXT)));
		text.child(UIComponents.label(Component.literal(server.ip)).color(Color.ofArgb(Ui.TEXT_FAINT)));
		text.child(UIComponents.label(Component.literal(Status.line(state))).color(Color.ofArgb(Ui.TEXT_DIM)));
		entry.child(text);

		entry.child(UIComponents.button(Component.literal("Mods"),
						pressed -> this.minecraft.setScreen(new ModBrowserScreen(this, server.ip)))
				.sizing(Sizing.fixed(48), Sizing.fixed(18)));
		return entry;
	}

	private void join(ServerData server) {
		ConnectScreen.startConnecting(this, this.minecraft, ServerAddress.parseString(server.ip),
				server, false, null);
	}
}
