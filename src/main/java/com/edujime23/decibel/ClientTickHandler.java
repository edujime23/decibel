package com.edujime23.decibel;

import com.edujime23.decibel.asm.SoundInterceptor;
import net.minecraft.client.Minecraft;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.neoforge.client.event.ClientPlayerNetworkEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.event.level.LevelEvent;

import java.util.Locale;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class ClientTickHandler {

    private static final ExecutorService GEOMETRY_WORKER = Executors.newSingleThreadExecutor(r -> {
        Thread thread = new Thread(r, "Decibel-Geometry-Worker");
        thread.setDaemon(true);
        thread.setPriority(Thread.NORM_PRIORITY - 1);
        return thread;
    });

    private static long lastUpdateTime = 0;
    private static final byte[] VOXEL_CACHE = new byte[64 * 64 * 64];

    @SubscribeEvent
    public static void onClientTick(ClientTickEvent.Post event) {
        SoundInterceptor.syncGlobalState();

        Minecraft mc = Minecraft.getInstance();
        if (mc.player != null && mc.level != null && DaemonManager.ipc != null) {
            long currentTime = System.currentTimeMillis();

            // Continuous poll throttled at 100ms: Instantly captures block breaks, places, and explosions [7.1]
            if (currentTime - lastUpdateTime >= 100) {
                lastUpdateTime = currentTime;
                BlockPos currentPos = mc.player.blockPosition();
                Level level = mc.level;

                GEOMETRY_WORKER.submit(() -> rebuildLocalAcoustics(level, currentPos));
            }
        }
    }

    private static void rebuildLocalAcoustics(Level level, BlockPos center) {
        // Expand sweep to 64x64x64 blocks centered on the player [7.1]
        int startX = center.getX() - 32;
        int startY = center.getY() - 32;
        int startZ = center.getZ() - 32;

        BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();

        for (int x = 0; x < 64; x++) {
            for (int y = 0; y < 64; y++) {
                for (int z = 0; z < 64; z++) {
                    pos.set(startX + x, startY + y, startZ + z);

                    if (level.hasChunkAt(pos)) {
                        BlockState state = level.getBlockState(pos);
                        VOXEL_CACHE[(x * 4096) + (y * 64) + z] = getAcousticMaterialId(state);
                    } else {
                        VOXEL_CACHE[(x * 4096) + (y * 64) + z] = 0; // Boundary defaults to AIR
                    }
                }
            }
        }

        DaemonManager.ipc.updateVoxelGrid(VOXEL_CACHE, startX, startY, startZ);
    }

    private static byte getAcousticMaterialId(BlockState state) {
        if (state.isAir()) {
            return 0; // AIR
        }

        // Culling fluids to preserve clean ray segments [7.3]
        if (!state.getFluidState().isEmpty()) {
            return 0;
        }

        String blockId = BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString().toLowerCase(Locale.ROOT);
        return MaterialRegistry.getMaterialId(blockId);
    }

    @SubscribeEvent
    public static void onPlayerLoggedOut(ClientPlayerNetworkEvent.LoggingOut event) {
        SoundInterceptor.forceStopAll();
        lastUpdateTime = 0;
    }

    @SubscribeEvent
    public static void onLevelUnload(LevelEvent.Unload event) {
        if (event.getLevel().isClientSide()) {
            SoundInterceptor.forceStopAll();
            lastUpdateTime = 0;
        }
    }
}