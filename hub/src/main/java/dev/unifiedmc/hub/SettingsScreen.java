package dev.unifiedmc.hub;

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
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;
import org.jetbrains.annotations.NotNull;

/** The few things about the hub worth deciding: how big it is, and whether it makes noise. */
public class SettingsScreen extends BaseOwoScreen<FlowLayout> {
	private final Screen parent;

	public SettingsScreen(Screen parent) {
		this.parent = parent;
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

		FlowLayout card = UIContainers.verticalFlow(Sizing.content(), Sizing.content());
		card.gap(10);
		card.surface(Surface.flat(Ui.CARD_TOP).and(Surface.outline(Ui.EDGE)));
		card.padding(Insets.of(16));
		card.horizontalAlignment(HorizontalAlignment.CENTER);

		card.child(UIComponents.label(Component.literal("Einstellungen")).color(Color.ofArgb(Ui.TEXT)));

		card.child(UIComponents.label(Component.literal("Groesse")).color(Color.ofArgb(Ui.TEXT_DIM)));
		FlowLayout scales = UIContainers.horizontalFlow(Sizing.content(), Sizing.content());
		scales.gap(4);
		for (int option : Settings.SCALES) {
			int value = option;
			boolean active = Settings.scale() == value;
			String label = value == Settings.SCALE_AUTO ? "Auto" : value + "x";
			var button = UIComponents.button(Component.literal(active ? "> " + label + " <" : label),
					pressed -> {
						Settings.set(value, Settings.muted());
						this.minecraft.setScreen(new SettingsScreen(parent));
					});
			button.sizing(Sizing.fixed(52), Sizing.fixed(20));
			scales.child(button);
		}
		card.child(scales);

		card.child(UIComponents.label(Component.literal("Arbeitsspeicher"))
				.color(Color.ofArgb(Ui.TEXT_DIM)));
		FlowLayout memory = UIContainers.horizontalFlow(Sizing.content(), Sizing.content());
		memory.gap(4);
		for (int option : Settings.RAM) {
			int value = option;
			boolean active = Settings.ram() == value;
			String label = value == Settings.RAM_AUTO ? "Auto" : (value / 1024) + " GB";
			var button = UIComponents.button(Component.literal(active ? "> " + label + " <" : label),
					pressed -> {
						Settings.setRam(value);
						this.minecraft.setScreen(new SettingsScreen(parent));
					});
			button.sizing(Sizing.fixed(52), Sizing.fixed(20));
			memory.child(button);
		}
		card.child(memory);
		card.child(UIComponents.label(Component.literal(
						"Auto richtet sich nach der Packgroesse  ·  gilt ab dem naechsten Start"))
				.color(Color.ofArgb(Ui.TEXT_FAINT)));

		card.child(UIComponents.button(
						Component.literal(Settings.muted() ? "Ton: aus" : "Ton: an"),
						pressed -> {
							Settings.set(Settings.scale(), !Settings.muted());
							this.minecraft.setScreen(new SettingsScreen(parent));
						})
				.sizing(Sizing.fixed(140), Sizing.fixed(20)));

		card.child(UIComponents.button(Component.literal("Zurueck"),
						pressed -> this.minecraft.setScreen(parent))
				.sizing(Sizing.fixed(140), Sizing.fixed(20)));

		root.child(card);
	}
}
