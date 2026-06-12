package com.edujime23.decibel.virtual;

import com.edujime23.decibel.daemon.DaemonManager;
import com.edujime23.decibel.Decibel;
import java.nio.ByteBuffer;
import java.util.Collections;
import java.util.Map;
import java.util.WeakHashMap;
import java.util.concurrent.atomic.AtomicInteger;
import javax.sound.sampled.AudioFormat;
import net.minecraft.client.sounds.AudioStream;

public class VirtualChannel {
    private static final AtomicInteger channelIdGenerator = new AtomicInteger(5000);
    private static final Map<Object, VirtualChannel> activeChannels = Collections.synchronizedMap(new WeakHashMap<>());

    private final int uid;
    private float x, y, z;
    private float volume = 1.0f;
    private float pitch = 1.0f;
    private boolean relative = false;
    private boolean spatial = true;
    private boolean looping = false;
    private boolean playing = false;

    private AudioStream stream;
    private boolean isStream = false;

    public VirtualChannel() {
        this.uid = channelIdGenerator.incrementAndGet();
    }

    public static void register(Object channelRef) {
        activeChannels.put(channelRef, new VirtualChannel());
    }

    public static VirtualChannel get(Object channelRef) {
        return activeChannels.computeIfAbsent(channelRef, k -> new VirtualChannel());
    }

    public static void play(Object channelRef) { get(channelRef).play(); }
    public static void stop(Object channelRef) { get(channelRef).stop(); }
    public static void pause(Object channelRef) { get(channelRef).pause(); }
    public static void unpause(Object channelRef) { get(channelRef).play(); }
    public static void disableAttenuation(Object channelRef) { get(channelRef).spatial = false; }

    public static void release(Object channelRef) {
        VirtualChannel vc = activeChannels.remove(channelRef);
        if (vc != null) vc.stop();
    }

    public static boolean isPlaying(Object channelRef) { return get(channelRef).playing; }
    public static boolean isStopped(Object channelRef) { return !get(channelRef).playing; }

    public static void attachStaticBuffer(Object channelRef, Object bufferRef) { }

    public static void attachBufferStream(Object channelRef, AudioStream stream) {
        get(channelRef).attachBufferStream(stream);
    }

    public static void setSelfPosition(Object channelRef, double x, double y, double z) {
        get(channelRef).setSelfPosition((float) x, (float) y, (float) z);
    }

    public static void setPitch(Object channelRef, float pitch) { get(channelRef).pitch = pitch; }
    public static void setVolume(Object channelRef, float volume) { get(channelRef).volume = volume; }
    public static void setAttenuation(Object channelRef, float attenuation) { /* Managed natively */ }
    public static void setLooping(Object channelRef, boolean looping) { get(channelRef).looping = looping; }
    public static void setRelative(Object channelRef, boolean relative) { get(channelRef).relative = relative; }
    public static void pumpBuffers(Object channelRef, int count) { get(channelRef).pumpBuffers(count); }
    public static void updateStream(Object channelRef) {
        VirtualChannel vc = activeChannels.get(channelRef);
        if (vc == null || !vc.playing || !vc.isStream) return;
        vc.pumpBuffers(1);
    }

    public void attachBufferStream(AudioStream stream) {
        this.stream = stream;
        this.isStream = true;
    }

    private void dispatchStreamPlay() {
        if (!this.isStream || DaemonManager.channel == null) return;

        int sampleRate = 48000;
        int channels = 1;

        if (this.stream != null && this.stream.getFormat() != null) {
            sampleRate = (int) this.stream.getFormat().getSampleRate();
            channels = this.stream.getFormat().getChannels();
        }

        DaemonManager.channel.sendPlayStream(uid, x, y, z, volume, pitch, relative, spatial, 0, sampleRate, channels);
    }

    public void play() {
        this.playing = true;
        dispatchStreamPlay();
    }

    public void stop() {
        this.playing = false;
        if (DaemonManager.ipc != null) DaemonManager.ipc.writeStopEvent(uid);
    }

    public void pause() {
        this.playing = false;
        if (DaemonManager.ipc != null) DaemonManager.ipc.writeStopEvent(uid);
    }

    public void setSelfPosition(float x, float y, float z) {
        this.x = x;
        this.y = y;
        this.z = z;
        this.spatial = true;
        if (this.playing) {
            dispatchStreamPlay();
        }
    }

    public void pumpBuffers(int count) {
        if (!this.playing || stream == null || DaemonManager.channel == null) return;
        try {
            AudioFormat format = stream.getFormat();
            if (format == null) return;

            int sampleSize = format.getSampleSizeInBits();
            boolean bigEndian = format.isBigEndian();

            for (int i = 0; i < count; i++) {
                ByteBuffer bytes = stream.read(8192);
                if (bytes == null || !bytes.hasRemaining()) break;

                byte[] data = new byte[bytes.remaining()];
                bytes.get(data);

                float[] floats;
                if (sampleSize == 8) {
                    floats = new float[data.length];
                    for (int idx = 0; idx < data.length; idx++) {
                        floats[idx] = data[idx] / 128.0f;
                    }
                } else {
                    floats = new float[data.length / 2];
                    for (int idx = 0; idx < floats.length; idx++) {
                        int b1 = data[idx * 2] & 0xFF;
                        int b2 = data[idx * 2 + 1] & 0xFF;
                        short sample = bigEndian ? (short) ((b1 << 8) | b2) : (short) ((b2 << 8) | b1);
                        floats[idx] = sample / 32768.0f;
                    }
                }

                if (floats.length > 0) {
                    DaemonManager.channel.sendStreamData(uid, floats);
                }
            }
        } catch (Exception e) {
            // Log once and suppress to avoid console spam lag
            this.playing = false;
            Decibel.LOGGER.error("Stream pump failed, disabling virtual channel stream", e);
        }
    }
}