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

    // --- STREAM FLOW CONTROL CLOCK ---
    private long playTimeNs = 0;       // Accumulated active playing duration
    private long lastTickNs = 0;       // Real system time stamp of our last update
    private long actualFramesSent = 0; // Cumulative frames pushed to the native side

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
        this.playTimeNs = 0;
        this.actualFramesSent = 0;
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
        if (this.playing) return;
        this.playing = true;

        // Reset ticker timeline anchor so we don't skew the delta
        this.lastTickNs = System.nanoTime();

        dispatchStreamPlay();
    }

    public void stop() {
        this.playing = false;
        this.playTimeNs = 0;
        this.actualFramesSent = 0;
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
        if (this.playing && DaemonManager.ipc != null) {
            DaemonManager.ipc.writeUpdatePosEvent(uid, x, y, z);
        }
    }

    public void pumpBuffers(int count) {
        if (!this.playing || stream == null || DaemonManager.channel == null) return;
        try {
            AudioFormat format = stream.getFormat();
            if (format == null) return;

            int sampleRate = (int) format.getSampleRate();
            int channels = format.getChannels();
            int sampleSize = format.getSampleSizeInBits();
            boolean bigEndian = format.isBigEndian();

            // 1. Progress virtual playback clock
            long now = System.nanoTime();
            this.playTimeNs += (now - this.lastTickNs);
            this.lastTickNs = now;

            // 2. Map playhead frames to wall clock elapsed play duration
            double elapsedSeconds = this.playTimeNs / 1_000_000_000.0;
            double expectedFramesPlayed = elapsedSeconds * sampleRate;

            // Safety cushion (4096 frames = ~85ms of jitter buffer safety)
            double targetFramesSent = expectedFramesPlayed + 4096;

            int loops = 0;
            // 3. Regulate socket data flow strictly against target play cushion
            while (this.actualFramesSent < targetFramesSent && loops < 16) {
                loops++;

                // Read in 2,048 byte blocks (exactly 512 stereo frames of 16-bit PCM)
                ByteBuffer bytes = stream.read(2048);
                if (bytes == null || !bytes.hasRemaining()) {
                    break;
                }

                byte[] data = new byte[bytes.remaining()];
                bytes.get(data);

                float[] floats;
                if (sampleSize == 8) {
                    floats = new float[data.length];
                    for (int idx = 0; idx < data.length; idx++) {
                        // Normalize unsigned 8-bit bias
                        floats[idx] = ((data[idx] & 0xFF) - 128) / 128.0f;
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
                    int framesRead = floats.length / channels;
                    this.actualFramesSent += framesRead;
                    DaemonManager.channel.sendStreamData(uid, floats);
                }
            }
        } catch (Exception e) {
            this.playing = false;
            Decibel.LOGGER.error("Stream pump failed, disabling virtual channel stream", e);
        }
    }
}