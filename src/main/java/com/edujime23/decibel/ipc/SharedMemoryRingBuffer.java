package com.edujime23.decibel.ipc;

import java.io.File;
import java.io.RandomAccessFile;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.nio.ByteOrder;
import java.nio.MappedByteBuffer;
import java.nio.channels.FileChannel;
import java.util.concurrent.atomic.AtomicInteger;

public class SharedMemoryRingBuffer {
    public static final int QUEUE_CAPACITY = 1024;
    public static final int SLOT_SIZE = 64;

    private static final int OFFSET_JAVA_WRITE_SEQ = 0;
    private static final int OFFSET_RUST_READ_SEQ = 64;
    private static final int OFFSET_VER = 128;
    private static final int OFFSET_DEV_SEQ = 192;
    private static final int OFFSET_DEV_NAME = 196;

    private static final int OFFSET_VOXEL_GRID_VERSION = 320;
    private static final int OFFSET_CENTER_X = 324;
    private static final int OFFSET_CENTER_Y = 328;
    private static final int OFFSET_CENTER_Z = 332;

    private static final int HEADER_SIZE = 512;
    private static final int RING_BUFFER_SIZE = QUEUE_CAPACITY * SLOT_SIZE;

    private static final int OFFSET_VOXEL_GRID = HEADER_SIZE + RING_BUFFER_SIZE; // 66,048
    private static final int VOXEL_GRID_SIZE = 64 * 64 * 64; // 262,144 bytes

    public static final int SHM_SIZE = OFFSET_VOXEL_GRID + VOXEL_GRID_SIZE; // 328,192 bytes

    private final MappedByteBuffer buffer;
    private final AtomicInteger writeSequence = new AtomicInteger(0);
    private int lastDeviceSeq = 0;
    private String lastSentDevice = "";
    private int voxelVersion = 0;

    private static final VarHandle INT_VIEW = MethodHandles.byteBufferViewVarHandle(int[].class, ByteOrder.LITTLE_ENDIAN);
    private static final VarHandle FLOAT_VIEW = MethodHandles.byteBufferViewVarHandle(float[].class, ByteOrder.LITTLE_ENDIAN);

    public SharedMemoryRingBuffer(File shmFile) throws Exception {
        try (RandomAccessFile memoryFile = new RandomAccessFile(shmFile, "rw")) {
            memoryFile.setLength(SHM_SIZE);
            this.buffer = memoryFile.getChannel().map(FileChannel.MapMode.READ_WRITE, 0, SHM_SIZE);
            this.buffer.order(ByteOrder.LITTLE_ENDIAN);

            INT_VIEW.setRelease(buffer, OFFSET_JAVA_WRITE_SEQ, 0);
            INT_VIEW.setRelease(buffer, OFFSET_RUST_READ_SEQ, 0);
            INT_VIEW.setRelease(buffer, OFFSET_VER, 0);
            INT_VIEW.setRelease(buffer, OFFSET_DEV_SEQ, 0);
            INT_VIEW.setRelease(buffer, OFFSET_VOXEL_GRID_VERSION, 0);

            for (int i = 12; i < HEADER_SIZE; i += 4) {
                if (i != OFFSET_RUST_READ_SEQ && i != OFFSET_VER && i != OFFSET_DEV_SEQ && i != OFFSET_VOXEL_GRID_VERSION) {
                    buffer.putFloat(i, 0.0f);
                }
            }
        }
    }

    public synchronized void updateVoxelGrid(byte[] voxelData, int centerX, int centerY, int centerZ) {
        if (voxelData.length != VOXEL_GRID_SIZE) {
            throw new IllegalArgumentException("Voxel grid data must be exactly 262144 bytes.");
        }

        int startPos = buffer.position();
        buffer.position(OFFSET_VOXEL_GRID);
        buffer.put(voxelData);
        buffer.position(startPos);

        INT_VIEW.setRelease(buffer, OFFSET_CENTER_X, centerX);
        INT_VIEW.setRelease(buffer, OFFSET_CENTER_Y, centerY);
        INT_VIEW.setRelease(buffer, OFFSET_CENTER_Z, centerZ);

        voxelVersion++;
        INT_VIEW.setRelease(buffer, OFFSET_VOXEL_GRID_VERSION, voxelVersion);
    }

    public synchronized void updateListener(float x, float y, float z, float fX, float fY, float fZ, float uX, float uY, float uZ) {
        int ver = (int) INT_VIEW.getAcquire(buffer, OFFSET_VER);
        INT_VIEW.setRelease(buffer, OFFSET_VER, ver + 1);

        FLOAT_VIEW.setRelease(buffer, 12, x);
        FLOAT_VIEW.setRelease(buffer, 16, y);
        FLOAT_VIEW.setRelease(buffer, 20, z);
        FLOAT_VIEW.setRelease(buffer, 24, fX);
        FLOAT_VIEW.setRelease(buffer, 28, fY);
        FLOAT_VIEW.setRelease(buffer, 32, fZ);
        FLOAT_VIEW.setRelease(buffer, 36, uX);
        FLOAT_VIEW.setRelease(buffer, 40, uY);
        FLOAT_VIEW.setRelease(buffer, 44, uZ);

        INT_VIEW.setRelease(buffer, OFFSET_VER, ver + 2);
    }

    public synchronized void updateGlobalState(float[] categoryVols, int flags) {
        int ver = (int) INT_VIEW.getAcquire(buffer, OFFSET_VER);
        INT_VIEW.setRelease(buffer, OFFSET_VER, ver + 1);

        int volOffset = 48;
        for (int i = 0; i < 16; i++) {
            float vol = (i < categoryVols.length) ? categoryVols[i] : 1.0f;
            FLOAT_VIEW.setRelease(buffer, volOffset + (i * 4), vol);
        }

        INT_VIEW.setRelease(buffer, 112, flags);
        INT_VIEW.setRelease(buffer, OFFSET_VER, ver + 2);
    }

    public synchronized void updateOutputDevice(String deviceName) {
        if (deviceName == null || deviceName.equals(lastSentDevice)) return;

        byte[] bytes = deviceName.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        int writeLen = Math.min(bytes.length, 127);

        int nameOffset = OFFSET_DEV_NAME;
        for (int i = 0; i < writeLen; i++) {
            buffer.put(nameOffset + i, bytes[i]);
        }
        buffer.put(nameOffset + writeLen, (byte) 0);

        lastDeviceSeq++;
        INT_VIEW.setRelease(buffer, OFFSET_DEV_SEQ, lastDeviceSeq);
        lastSentDevice = deviceName;
    }

    public boolean writePlayEvent(int uid, float x, float y, float z, float volume, float pitch, int assetHash, boolean relative, boolean spatial, int categoryId) {
        int seq = writeSequence.get();
        int slotIndex = seq % QUEUE_CAPACITY;
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        int rustReadSeq = (int) INT_VIEW.getAcquire(buffer, OFFSET_RUST_READ_SEQ);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) {
            return false;
        }

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_PLAY);
        INT_VIEW.setRelease(buffer, offset + 4, uid);
        FLOAT_VIEW.setRelease(buffer, offset + 8, x);
        FLOAT_VIEW.setRelease(buffer, offset + 12, y);
        FLOAT_VIEW.setRelease(buffer, offset + 16, z);
        FLOAT_VIEW.setRelease(buffer, offset + 20, volume);
        FLOAT_VIEW.setRelease(buffer, offset + 24, pitch);
        INT_VIEW.setRelease(buffer, offset + 28, assetHash);

        buffer.put(offset + 32, (byte) (relative ? 1 : 0));
        buffer.put(offset + 33, (byte) (spatial ? 1 : 0));
        buffer.put(offset + 34, (byte) categoryId);

        INT_VIEW.setRelease(buffer, OFFSET_JAVA_WRITE_SEQ, seq + 1);
        writeSequence.incrementAndGet();

        return true;
    }

    public boolean writeStopEvent(int uid) {
        int seq = writeSequence.get();
        int slotIndex = seq % QUEUE_CAPACITY;
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        int rustReadSeq = (int) INT_VIEW.getAcquire(buffer, OFFSET_RUST_READ_SEQ);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) {
            return false;
        }

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_STOP);
        INT_VIEW.setRelease(buffer, offset + 4, uid);
        for (int i = 8; i < SLOT_SIZE; i += 4) {
            INT_VIEW.setRelease(buffer, offset + i, 0);
        }

        INT_VIEW.setRelease(buffer, OFFSET_JAVA_WRITE_SEQ, seq + 1);
        writeSequence.incrementAndGet();

        return true;
    }

    public boolean writeStopAllEvent() {
        int seq = writeSequence.get();
        int slotIndex = seq % QUEUE_CAPACITY;
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        int rustReadSeq = (int) INT_VIEW.getAcquire(buffer, OFFSET_RUST_READ_SEQ);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) {
            return false;
        }

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_STOP_ALL);
        for (int i = 4; i < SLOT_SIZE; i += 4) {
            INT_VIEW.setRelease(buffer, offset + i, 0);
        }

        INT_VIEW.setRelease(buffer, OFFSET_JAVA_WRITE_SEQ, seq + 1);
        writeSequence.incrementAndGet();

        return true;
    }
}