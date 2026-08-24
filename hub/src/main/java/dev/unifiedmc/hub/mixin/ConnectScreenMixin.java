package dev.unifiedmc.hub.mixin;

import dev.unifiedmc.hub.Handoff;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.TransferState;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * The one funnel every connection attempt goes through - server list, direct connect, quick play
 * and transfers alike. One hook covers all of them.
 */
@Mixin(ConnectScreen.class)
public class ConnectScreenMixin {
	@Inject(method = "startConnecting", at = @At("HEAD"), cancellable = true)
	private static void unifiedmc$handoff(Screen parent, Minecraft minecraft, ServerAddress address,
			ServerData serverData, boolean quickPlay, TransferState transferState, CallbackInfo ci) {
		if (Handoff.request(address.getHost(), address.getPort())) {
			ci.cancel();
		}
	}
}
