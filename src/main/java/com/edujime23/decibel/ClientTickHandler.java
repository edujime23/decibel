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

    private static BlockPos lastCentroid = null;
    private static final byte[] VOXEL_CACHE = new byte[32 * 32 * 32];

    @SubscribeEvent
    public static void onClientTick(ClientTickEvent.Post event) {
        SoundInterceptor.syncGlobalState();

        Minecraft mc = Minecraft.getInstance();
        if (mc.player != null && mc.level != null && DaemonManager.ipc != null) {
            BlockPos currentPos = mc.player.blockPosition();

            // Safe Distance Check: Immune to mappings, Mojang releases, or Yarn versions
            boolean shouldUpdate = false;
            if (lastCentroid == null) {
                shouldUpdate = true;
            } else {
                int dx = lastCentroid.getX() - currentPos.getX();
                int dy = lastCentroid.getY() - currentPos.getY();
                int dz = lastCentroid.getZ() - currentPos.getZ();
                int distanceSquared = (dx * dx) + (dy * dy) + (dz * dz);
                if (distanceSquared >= 1) {
                    shouldUpdate = true;
                }
            }

            if (shouldUpdate) {
                lastCentroid = currentPos;
                Level level = mc.level;

                GEOMETRY_WORKER.submit(() -> rebuildLocalAcoustics(level, currentPos));
            }
        }
    }

    private static void rebuildLocalAcoustics(Level level, BlockPos center) {
        int startX = center.getX() - 16;
        int startY = center.getY() - 16;
        int startZ = center.getZ() - 16;

        BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();

        for (int x = 0; x < 32; x++) {
            for (int y = 0; y < 32; y++) {
                for (int z = 0; z < 32; z++) {
                    pos.set(startX + x, startY + y, startZ + z);

                    if (level.hasChunkAt(pos)) {
                        BlockState state = level.getBlockState(pos);
                        VOXEL_CACHE[(x * 1024) + (y * 32) + z] = getAcousticMaterialId(state);
                    } else {
                        VOXEL_CACHE[(x * 1024) + (y * 32) + z] = 0; // Boundary defaults to AIR
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

        // Culling fluid states to keep structural paths clean [7.3]
        if (!state.getFluidState().isEmpty()) {
            return 0;
        }

        // Resolves standard namespace keys safely across all versions
        String blockId = BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString().toLowerCase(Locale.ROOT);
        return MaterialRegistry.getMaterialId(blockId);
    }

    @SubscribeEvent
    public static void onPlayerLoggedOut(ClientPlayerNetworkEvent.LoggingOut event) {
        SoundInterceptor.forceStopAll();
        lastCentroid = null;
    }

    @SubscribeEvent
    public static void onLevelUnload(LevelEvent.Unload event) {
        if (event.getLevel().isClientSide()) {
            SoundInterceptor.forceStopAll();
            lastCentroid = null;
        }
    }
}