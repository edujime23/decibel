package com.edujime23.decibel.voxels;

import com.edujime23.decibel.asm.SoundInterceptor;
import com.edujime23.decibel.daemon.DaemonManager;
import net.minecraft.client.Minecraft;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.neoforge.client.event.ClientPlayerNetworkEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.event.level.LevelEvent;

public class ClientTickHandler {
    @SubscribeEvent
    public static void onClientTick(ClientTickEvent.Post event) {
        if (DaemonManager.ipc == null) return;

        SoundInterceptor.syncGlobalState();

        Minecraft mc = Minecraft.getInstance();
        if (mc.player == null || mc.level == null) return;

        VoxelCompiler.tick(mc.player.blockPosition(), mc.level);
    }

    @SubscribeEvent
    public static void onPlayerLoggedOut(ClientPlayerNetworkEvent.LoggingOut event) {
        SoundInterceptor.forceStopAll();
        VoxelCompiler.reset();
    }

    @SubscribeEvent
    public static void onLevelUnload(LevelEvent.Unload event) {
        if (!event.getLevel().isClientSide()) return;
        SoundInterceptor.forceStopAll();
        VoxelCompiler.reset();
    }
}