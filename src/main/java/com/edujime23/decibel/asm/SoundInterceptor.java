package com.edujime23.decibel.asm;

import com.edujime23.decibel.AssetCacher;
import com.edujime23.decibel.DaemonManager;
import net.minecraft.client.Camera;
import net.minecraft.client.Minecraft;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.resources.sounds.Sound;
import net.minecraft.client.sounds.WeighedSoundEvents;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3f;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public class SoundInterceptor {
    private static final Logger LOGGER = LoggerFactory.getLogger("Decibel-Interceptor");
    private static final java.util.concurrent.atomic.AtomicInteger soundIdGenerator = new java.util.concurrent.atomic.AtomicInteger(1);
    private static final java.util.concurrent.ConcurrentHashMap<SoundInstance, Integer> activeSoundUids = new java.util.concurrent.ConcurrentHashMap<>();

    public static boolean onPlaySound(SoundInstance sound) {
        if (sound == null) {
            return false;
        }

        try {
            if (sound.getSound() == null) {
                WeighedSoundEvents resolved = sound.resolve(Minecraft.getInstance().getSoundManager());
                if (resolved == null || sound.getSound() == null) {
                    return false;
                }
            }

            Sound soundRecord = sound.getSound();

            if (DaemonManager.ipc == null) {
                return false;
            }

            // Resolve final volume options multipliers using Mojang's getSource() method
            float masterVolume = Minecraft.getInstance().options.getSoundSourceVolume(net.minecraft.sounds.SoundSource.MASTER);
            float categoryVolume = Minecraft.getInstance().options.getSoundSourceVolume(sound.getSource());
            float finalVolume = sound.getVolume() * categoryVolume * masterVolume;

            boolean relative = sound.isRelative();
            boolean spatial = sound.getAttenuation() != SoundInstance.Attenuation.NONE;

            // Diagnostic Logger: Prints computed audio vectors in the console
            LOGGER.info("DECIBEL EVENT -> Location: {} | Source: {} | BaseVol: {} | CatVol: {} | Master: {} | FinalVol: {} | Relative: {} | Spatial: {}",
                soundRecord.getLocation(), sound.getSource(), sound.getVolume(), categoryVolume, masterVolume, finalVolume, relative, spatial);

            // Short-circuit if muted (prevents sending muted events to Rust)
            if (finalVolume <= 0.0001f) {
                return true;
            }

            int uid = soundIdGenerator.incrementAndGet();
            activeSoundUids.put(sound, uid);

            float x = (float) sound.getX();
            float y = (float) sound.getY();
            float z = (float) sound.getZ();
            float pitch = sound.getPitch();

            int assetHash = soundRecord.getLocation().toString().hashCode();
            AssetCacher.ensureCached(soundRecord.getLocation(), assetHash);

            boolean sent = DaemonManager.ipc.writePlayEvent(uid, x, y, z, finalVolume, pitch, assetHash, relative, spatial);
            return sent;

        } catch (Throwable t) {
            LOGGER.error("Failed to process spatial sound in interceptor", t);
            return false;
        }
    }

    public static boolean onStopSound(SoundInstance sound) {
        if (sound == null || DaemonManager.ipc == null) {
            return false;
        }
        try {
            Integer uid = activeSoundUids.remove(sound);
            if (uid != null) {
                return DaemonManager.ipc.writeStopEvent(uid);
            }
        } catch (Throwable t) {
            LOGGER.error("Failed to process stop sound in interceptor", t);
        }
        return false;
    }

    public static void onUpdateListener(Camera camera) {
        if (camera == null || DaemonManager.ipc == null) {
            return;
        }
        try {
            Vec3 pos = camera.getPosition();
            Vector3f fwd = camera.getLookVector();
            Vector3f up = camera.getUpVector();

            DaemonManager.ipc.updateListener(
                (float) pos.x, (float) pos.y, (float) pos.z,
                fwd.x, fwd.y, fwd.z,
                up.x, up.y, up.z
            );
        } catch (Throwable t) {
            LOGGER.error("Failed to update spatial listener", t);
        }
    }
}