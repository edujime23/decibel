package com.edujime23.decibel;

import com.edujime23.decibel.ipc.DaemonChannel;
import net.minecraft.client.Minecraft;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.packs.resources.Resource;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CompletableFuture;

public class AssetCacher {
    private static final ConcurrentHashMap<Integer, Boolean> cachedAssets = new ConcurrentHashMap<>();
    private static DaemonChannel channel;

    public static void init(DaemonChannel ipcChannel) {
        channel = ipcChannel;
        cachedAssets.clear();
    }

    public static void ensureCached(ResourceLocation soundLocation, int assetHash) {
        if (cachedAssets.containsKey(assetHash) || channel == null) return;
        cachedAssets.put(assetHash, true); // Mark active immediately to prevent spam

        ResourceLocation oggLocation = ResourceLocation.fromNamespaceAndPath(soundLocation.getNamespace(), "sounds/" + soundLocation.getPath() + ".ogg");

        // Negative Space Fix: Completely async file streaming prevents Minecraft freeze on large tracks
        CompletableFuture.runAsync(() -> {
            try {
                Optional<Resource> resource = Minecraft.getInstance().getResourceManager().getResource(oggLocation);
                if (resource.isEmpty()) return;

                try (InputStream is = resource.get().open()) {
                    ByteArrayOutputStream bos = new ByteArrayOutputStream();
                    byte[] buf = new byte[8192];
                    int len;
                    while ((len = is.read(buf)) != -1) bos.write(buf, 0, len);
                    channel.sendAsset(assetHash, bos.toByteArray());
                }
            } catch (Exception e) {
                Decibel.LOGGER.error("Failed to stream asset: {}", oggLocation, e);
                cachedAssets.remove(assetHash);
            }
        });
    }
}