package dev.unifiedmc.hub;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.minecraft.client.gui.screens.TitleScreen;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The hub is a server list and nothing else.
 *
 * No mixin on purpose: Fabric's screen events already expose what we need, and an event survives
 * a Minecraft update that an injection point does not.
 */
public class UnifiedMcHub implements ClientModInitializer {
	public static final Logger LOG = LoggerFactory.getLogger("unifiedmc");

	/** Set once the server list has been shown, so a title screen after it means "back". */
	private boolean listWasShown;

	@Override
	public void onInitializeClient() {
		ClientLifecycleEvents.CLIENT_STARTED.register(client -> {
			Settings.apply(client);
			Handoff.publishSession(client);
			Handoff.publishServerList(client);
		});

		ScreenEvents.AFTER_INIT.register((client, screen, width, height) -> {
			if (screen instanceof TitleScreen) {
				// The title screen is never drawn. Reaching it the first time means startup and we
				// skip straight past it; reaching it again means the player pressed Back on the
				// only screen there is, and the only thing left of a launcher is to close it.
				if (listWasShown) {
					client.stop();
				} else {
					client.setScreen(new HubScreen());
				}
			} else if (screen instanceof HubScreen) {
				listWasShown = true;
			}
		});

		LOG.info("UnifiedMC hub ready");
	}

}
