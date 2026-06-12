package com.edujime23.decibel.voxels;

import com.edujime23.decibel.daemon.DaemonManager;
import com.edujime23.decibel.MaterialRegistry;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.core.registries.BuiltInRegistries;

import java.util.Locale;
import java.util.concurrent.CompletableFuture;

public class VoxelCompiler {
    private static final byte[] VOXEL_CACHE = new byte[64 * 64 * 64];
    private static int currentSliceX = 0;
    private static BlockPos lastCenterPos = null;

    public static void tick(BlockPos currentPos, Level level) {
        if (lastCenterPos == null || currentPos.distSqr(lastCenterPos) > 16) {
            lastCenterPos = currentPos.immutable();
            currentSliceX = 0;
        }

        final int sliceToProcess = currentSliceX;
        final int startX = lastCenterPos.getX() - 32;
        final int startY = lastCenterPos.getY() - 32;
        final int startZ = lastCenterPos.getZ() - 32;

        // Negative Space Fix: Offload chunk reading to avoid 16,000 main-thread blocks.
        CompletableFuture.runAsync(() -> {
            BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
            for (int i = 0; i < 4; i++) {
                int x = (sliceToProcess + i) % 64;
                int worldX = startX + x;

                for (int y = 0; y < 64; y++) {
                    int worldY = startY + y;
                    for (int z = 0; z < 64; z++) {
                        int worldZ = startZ + z;
                        pos.set(worldX, worldY, worldZ);

                        if (!level.hasChunkAt(pos)) {
                            VOXEL_CACHE[(x * 4096) + (y * 64) + z] = 0;
                            continue;
                        }

                        BlockState state = level.getBlockState(pos);
                        VOXEL_CACHE[(x * 4096) + (y * 64) + z] = getAcousticMaterialId(state);
                    }
                }
            }
        }).thenRun(() -> {
            if (sliceToProcess + 4 >= 64 && DaemonManager.ipc != null) {
                DaemonManager.ipc.updateVoxelGrid(VOXEL_CACHE, startX, startY, startZ);
            }
        });

        currentSliceX = (currentSliceX + 4) % 64;
    }

    public static void reset() {
        currentSliceX = 0;
        lastCenterPos = null;
    }

    private static byte getAcousticMaterialId(BlockState state) {
        if (state.isAir() || !state.getFluidState().isEmpty()) return 0;
        String blockId = BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString().toLowerCase(Locale.ROOT);
        return MaterialRegistry.getMaterialId(blockId);
    }
}