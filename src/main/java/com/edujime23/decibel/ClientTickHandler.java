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

public class ClientTickHandler {

    private static final byte[] VOXEL_CACHE = new byte[64 * 64 * 64];
    private static int currentSliceX = 0;
    private static BlockPos lastCenterPos = null;

    @SubscribeEvent
    public static void onClientTick(ClientTickEvent.Post event) {
        SoundInterceptor.syncGlobalState();

        Minecraft mc = Minecraft.getInstance();
        if (mc.player != null && mc.level != null && DaemonManager.ipc != null) {
            BlockPos currentPos = mc.player.blockPosition();
            Level level = mc.level;

            // If player moved significantly, reset slice sweep to start immediately
            if (lastCenterPos == null || currentPos.distSqr(lastCenterPos) > 16) {
                lastCenterPos = currentPos.immutable();
                currentSliceX = 0;
            }

            // Slice-by-slice sweep on main thread to guarantee thread-safety [7.1]
            // Sweeps 4 X-slices per tick (completely finished in 16 ticks / ~800ms)
            // This consumes less than 0.1ms of frame time, preventing tick micro-stutters
            int startX = lastCenterPos.getX() - 32;
            int startY = lastCenterPos.getY() - 32;
            int startZ = lastCenterPos.getZ() - 32;

            BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();

            for (int i = 0; i < 4; i++) {
                int x = (currentSliceX + i) % 64;
                int worldX = startX + x;

                for (int y = 0; y < 64; y++) {
                    int worldY = startY + y;
                    for (int z = 0; z < 64; z++) {
                        int worldZ = startZ + z;
                        pos.set(worldX, worldY, worldZ);

                        if (level.hasChunkAt(pos)) {
                            BlockState state = level.getBlockState(pos);
                            VOXEL_CACHE[(x * 4096) + (y * 64) + z] = getAcousticMaterialId(state);
                        } else {
                            VOXEL_CACHE[(x * 4096) + (y * 64) + z] = 0; // Default boundary to AIR
                        }
                    }
                }
            }

            currentSliceX = (currentSliceX + 4) % 64;

            // Push the fully compiled slices into the Shared Memory ring buffer
            if (currentSliceX == 0) {
                DaemonManager.ipc.updateVoxelGrid(VOXEL_CACHE, startX, startY, startZ);
            }
        }
    }

    private static byte getAcousticMaterialId(BlockState state) {
        if (state.isAir()) {
            return 0; // AIR
        }

        // Cull fluids to preserve clean ray segments [7.3]
        if (!state.getFluidState().isEmpty()) {
            return 0;
        }

        String blockId = BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString().toLowerCase(Locale.ROOT);
        return MaterialRegistry.getMaterialId(blockId);
    }

    @SubscribeEvent
    public static void onPlayerLoggedOut(ClientPlayerNetworkEvent.LoggingOut event) {
        SoundInterceptor.forceStopAll();
        currentSliceX = 0;
        lastCenterPos = null;
    }

    @SubscribeEvent
    public static void onLevelUnload(LevelEvent.Unload event) {
        if (event.getLevel().isClientSide()) {
            SoundInterceptor.forceStopAll();
            currentSliceX = 0;
            lastCenterPos = null;
        }
    }
}