package com.edujime23.decibel.ipc;

import java.io.File;
import java.io.RandomAccessFile;
import java.nio.ByteOrder;
import java.nio.MappedByteBuffer;
import java.nio.channels.FileChannel;
import java.util.concurrent.atomic.AtomicInteger;

public class SharedMemoryRingBuffer {
    public static final int QUEUE_CAPACITY = 1024;
    public static final int SLOT_SIZE = 64;
    private static final int HEADER_SIZE = 256;
    public static final int SHM_SIZE = HEADER_SIZE + (QUEUE_CAPACITY * SLOT_SIZE);

    private final MappedByteBuffer buffer;
    private final AtomicInteger writeSequence = new AtomicInteger(0);
    private int lastDeviceSeq = 0;
    private String lastSentDevice = "";

    public SharedMemoryRingBuffer(File shmFile) throws Exception {
        try (RandomAccessFile memoryFile = new RandomAccessFile(shmFile, "rw")) {
            memoryFile.setLength(SHM_SIZE);
            this.buffer = memoryFile.getChannel().map(FileChannel.MapMode.READ_WRITE, 0, SHM_SIZE);
            this.buffer.order(ByteOrder.LITTLE_ENDIAN);

            this.buffer.putInt(0, 0);
            this.buffer.putInt(4, 0);
            this.buffer.putInt(8, 0);

            for (int i = 12; i < HEADER_SIZE; i += 4) {
                this.buffer.putFloat(i, 0.0f);
            }
        }
    }

    public synchronized void updateListener(float x, float y, float z, float fX, float fY, float fZ, float uX, float uY, float uZ) {
        int ver = buffer.getInt(8);
        buffer.putInt(8, ver + 1);

        buffer.putFloat(12, x);
        buffer.putFloat(16, y);
        buffer.putFloat(20, z);
        buffer.putFloat(24, fX);
        buffer.putFloat(28, fY);
        buffer.putFloat(32, fZ);
        buffer.putFloat(36, uX);
        buffer.putFloat(40, uY);
        buffer.putFloat(44, uZ);

        buffer.putInt(8, ver + 2);
    }

    public synchronized void updateGlobalState(float[] categoryVols, int flags) {
        int ver = buffer.getInt(8);
        buffer.putInt(8, ver + 1);

        int volOffset = 48;
        for (int i = 0; i < 16; i++) {
            float vol = (i < categoryVols.length) ? categoryVols[i] : 1.0f;
            buffer.putFloat(volOffset + (i * 4), vol);
        }

        buffer.putInt(112, flags);

        buffer.putInt(8, ver + 2);
    }

    public synchronized void updateOutputDevice(String deviceName) {
        if (deviceName == null || deviceName.equals(lastSentDevice)) return;

        byte[] bytes = deviceName.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        int writeLen = Math.min(bytes.length, 127);

        int nameOffset = 120;
        for (int i = 0; i < writeLen; i++) {
            buffer.put(nameOffset + i, bytes[i]);
        }
        buffer.put(nameOffset + writeLen, (byte) 0);

        lastDeviceSeq++;
        buffer.putInt(116, lastDeviceSeq);
        lastSentDevice = deviceName;
    }

    public boolean writePlayEvent(int uid, float x, float y, float z, float volume, float pitch, int assetHash, boolean relative, boolean spatial, int categoryId) {
        int seq = writeSequence.get();
        int slotIndex = seq % QUEUE_CAPACITY;
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        int rustReadSeq = buffer.getInt(4);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) {
            return false;
        }

        buffer.putInt(offset, OpCodes.OP_PLAY);
        buffer.putInt(offset + 4, uid);
        buffer.putFloat(offset + 8, x);
        buffer.putFloat(offset + 12, y);
        buffer.putFloat(offset + 16, z);
        buffer.putFloat(offset + 20, volume);
        buffer.putFloat(offset + 24, pitch);
        buffer.putInt(offset + 28, assetHash);

        buffer.put(offset + 32, (byte) (relative ? 1 : 0));
        buffer.put(offset + 33, (byte) (spatial ? 1 : 0));
        buffer.put(offset + 34, (byte) categoryId);

        buffer.putInt(0, seq + 1);
        writeSequence.incrementAndGet();

        return true;
    }

    public boolean writeStopEvent(int uid) {
        int seq = writeSequence.get();
        int slotIndex = seq % QUEUE_CAPACITY;
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        int rustReadSeq = buffer.getInt(4);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) {
            return false;
        }

        buffer.putInt(offset, OpCodes.OP_STOP);
        buffer.putInt(offset + 4, uid);
        for (int i = 8; i < SLOT_SIZE; i += 4) {
            buffer.putInt(offset + i, 0);
        }

        buffer.putInt(0, seq + 1);
        writeSequence.incrementAndGet();

        return true;
    }

    public boolean writeStopAllEvent() {
        int seq = writeSequence.get();
        int slotIndex = seq % QUEUE_CAPACITY;
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        int rustReadSeq = buffer.getInt(4);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) {
            return false;
        }

        buffer.putInt(offset, OpCodes.OP_STOP_ALL);
        for (int i = 4; i < SLOT_SIZE; i += 4) {
            buffer.putInt(offset + i, 0);
        }

        buffer.putInt(0, seq + 1);
        writeSequence.incrementAndGet();

        return true;
    }
}