package dev.unifiedmc.hub.mixin;

import java.util.List;

import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.ServerList;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/**
 * Direct Connect does not add a visible server - it stores the address as a hidden entry, which
 * {@link ServerList#size()} does not count. That hidden list is exactly the server the player is
 * most likely to rejoin, so the shell has to see it too.
 */
@Mixin(ServerList.class)
public interface ServerListAccessor {
	@Accessor("hiddenServerList")
	List<ServerData> unifiedmc$hidden();
}
