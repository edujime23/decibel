package com.edujime23.decibel.asm;

import com.edujime23.decibel.AssetCacher;
import com.edujime23.decibel.Config;
import com.edujime23.decibel.DaemonManager;
import net.minecraft.client.Camera;
import net.minecraft.client.Minecraft;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.resources.sounds.Sound;
import net.minecraft.client.sounds.WeighedSoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3f;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.util.Collections;
import java.util.Map;
import java.util.WeakHashMap;

public class SoundInterceptor {
    private static final Logger LOGGER = LoggerFactory.getLogger("Decibel-Interceptor");
    private static final java.util.concurrent.atomic.AtomicInteger soundIdGenerator = new java.util.concurrent.atomic.AtomicInteger(1);

    // Solves Memory Leak: Using WeakHashMap prevents holding strong references to transient sounds
    private static final Map<SoundInstance, Integer> activeSoundUids = Collections.synchronizedMap(new WeakHashMap<>());

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

            float baseVolume = sound.getVolume();
            float pitch = sound.getPitch();

            boolean relative = sound.isRelative();
            boolean spatial = sound.getAttenuation() != SoundInstance.Attenuation.NONE;

            if (sound.getSource() == SoundSource.RECORDS) {
                relative = false;
                spatial = true;
            }

            if (baseVolume <= 0.0001f) {
                return true;
            }

            int uid = soundIdGenerator.incrementAndGet();
            activeSoundUids.put(sound, uid);

            float x = (float) sound.getX();
            float y = (float) sound.getY();
            float z = (float) sound.getZ();

            int assetHash = soundRecord.getLocation().toString().hashCode();
            AssetCacher.ensureCached(soundRecord.getLocation(), assetHash);

            int categoryId = sound.getSource().ordinal();

            boolean sent = DaemonManager.ipc.writePlayEvent(
                uid, x, y, z, baseVolume, pitch, assetHash, relative, spatial, categoryId
            );
            return sent;

        } catch (Throwable t) {
            LOGGER.error("Failed to process sound in interceptor", t);
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

    /**
     * Standard SoundEngine cleanup hook.
     * Retains the level == null safety guard to prevent options/video settings reloads
     * from silencing playing Jukebox tracks mid-game.
     */
    public static void onStopAll() {
        if (DaemonManager.ipc == null) {
            return;
        }
        try {
            if (Minecraft.getInstance().level == null) {
                forceStopAll();
            }
        } catch (Throwable t) {
            LOGGER.error("Failed to process stop-all sounds in interceptor", t);
        }
    }

    /**
     * Forces immediate stops of all active tracking registries and pushes the stop opcode to the daemon.
     * Used on true world disconnects and portal/dimension travel.
     */
    public static void forceStopAll() {
        if (DaemonManager.ipc == null) {
            return;
        }
        try {
            activeSoundUids.clear();
            DaemonManager.ipc.writeStopAllEvent();
        } catch (Throwable t) {
            LOGGER.error("Failed to force stop all active sounds on the daemon", t);
        }
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

    public static void syncGlobalState() {
        if (DaemonManager.ipc == null || Minecraft.getInstance() == null || Minecraft.getInstance().options == null) {
            return;
        }
        try {
            float[] categoryVols = new float[16];
            for (SoundSource source : SoundSource.values()) {
                categoryVols[source.ordinal()] = Minecraft.getInstance().options.getSoundSourceVolume(source);
            }

            boolean paused = Minecraft.getInstance().isPaused();
            boolean directionalAudio = Minecraft.getInstance().options.directionalAudio().get();

            int flags = 0;
            if (paused) flags |= (1 << 0);
            if (Config.ENABLE_STEAM_AUDIO.get() && directionalAudio) flags |= (1 << 1);
            if (Config.ENABLE_OCCLUSION.get()) flags |= (1 << 2);
            if (Config.ENABLE_TRANSMISSION.get()) flags |= (1 << 3);
            if (Config.ENABLE_REVERB.get()) flags |= (1 << 4);
            if (Config.ENABLE_REFLECTION.get()) flags |= (1 << 5);

            DaemonManager.ipc.updateGlobalState(categoryVols, flags);

            String currentDevice = Minecraft.getInstance().options.soundDevice().get();
            DaemonManager.ipc.updateOutputDevice(currentDevice);
        } catch (Throwable t) {
            // Ignore during setup/shutdown sequence
        }
    }
}