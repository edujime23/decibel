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

    // Defensively-spaced offsets to guarantee zero memory overlap
    private static final int OFFSET_JAVA_WRITE_SEQ = 0;
    private static final int OFFSET_RUST_READ_SEQ = 4;
    private static final int OFFSET_HEARTBEAT = 8;
    private static final int OFFSET_VER = 12;
    private static final int OFFSET_DEV_SEQ = 16;
    private static final int OFFSET_VOXEL_GRID_VERSION = 20;
    private static final int OFFSET_START_X = 24;
    private static final int OFFSET_START_Y = 28;
    private static final int OFFSET_START_Z = 32;
    private static final int OFFSET_FLAGS = 36;

    private static final int OFFSET_LISTENER_POS = 40;     // 12 bytes
    private static final int OFFSET_LISTENER_FWD = 52;     // 12 bytes
    private static final int OFFSET_LISTENER_UP = 64;      // 12 bytes
    private static final int OFFSET_CATEGORY_VOLUMES = 76; // 64 bytes (16 floats)
    private static final int OFFSET_DEV_NAME = 140;        // 128 bytes

    private static final int HEADER_SIZE = 512;
    private static final int RING_BUFFER_SIZE = QUEUE_CAPACITY * SLOT_SIZE;
    private static final int OFFSET_VOXEL_GRID = HEADER_SIZE + RING_BUFFER_SIZE;
    private static final int VOXEL_GRID_SIZE = 64 * 64 * 64;

    public static final int SHM_SIZE = OFFSET_VOXEL_GRID + VOXEL_GRID_SIZE;

    private final MappedByteBuffer buffer;
    private final AtomicInteger writeSequence = new AtomicInteger(0);
    private int lastDeviceSeq = 0;
    private String lastSentDevice = "";
    private int voxelVersion = 0;
    private int heartbeatCounter = 0;

    private static final VarHandle INT_VIEW = MethodHandles.byteBufferViewVarHandle(int[].class, ByteOrder.LITTLE_ENDIAN);
    private static final VarHandle FLOAT_VIEW = MethodHandles.byteBufferViewVarHandle(float[].class, ByteOrder.LITTLE_ENDIAN);

    public SharedMemoryRingBuffer(File shmFile) throws Exception {
        try (RandomAccessFile memoryFile = new RandomAccessFile(shmFile, "rw")) {
            memoryFile.setLength(SHM_SIZE);
            this.buffer = memoryFile.getChannel().map(FileChannel.MapMode.READ_WRITE, 0, SHM_SIZE);
            this.buffer.order(ByteOrder.LITTLE_ENDIAN);

            INT_VIEW.setRelease(buffer, OFFSET_JAVA_WRITE_SEQ, 0);
            INT_VIEW.setRelease(buffer, OFFSET_RUST_READ_SEQ, 0);
            INT_VIEW.setRelease(buffer, OFFSET_HEARTBEAT, 0);
            INT_VIEW.setRelease(buffer, OFFSET_VER, 0);
            INT_VIEW.setRelease(buffer, OFFSET_DEV_SEQ, 0);
            INT_VIEW.setRelease(buffer, OFFSET_VOXEL_GRID_VERSION, 0);
            INT_VIEW.setRelease(buffer, OFFSET_START_X, 0);
            INT_VIEW.setRelease(buffer, OFFSET_START_Y, 0);
            INT_VIEW.setRelease(buffer, OFFSET_START_Z, 0);
            INT_VIEW.setRelease(buffer, OFFSET_FLAGS, 0);
        }
    }

    public void writeHeartbeat() {
        heartbeatCounter++;
        INT_VIEW.setRelease(buffer, OFFSET_HEARTBEAT, heartbeatCounter);
    }

    public synchronized void updateVoxelGrid(byte[] voxelData, int startX, int startY, int startZ) {
        if (voxelData.length != VOXEL_GRID_SIZE) return;
        int startPos = buffer.position();
        buffer.position(OFFSET_VOXEL_GRID);
        buffer.put(voxelData);
        buffer.position(startPos);

        INT_VIEW.setRelease(buffer, OFFSET_START_X, startX);
        INT_VIEW.setRelease(buffer, OFFSET_START_Y, startY);
        INT_VIEW.setRelease(buffer, OFFSET_START_Z, startZ);

        voxelVersion++;
        INT_VIEW.setRelease(buffer, OFFSET_VOXEL_GRID_VERSION, voxelVersion);
    }

    public synchronized void updateListener(float x, float y, float z, float fX, float fY, float fZ, float uX, float uY, float uZ) {
        int ver = (int) INT_VIEW.getAcquire(buffer, OFFSET_VER);
        INT_VIEW.setRelease(buffer, OFFSET_VER, ver + 1);

        FLOAT_VIEW.setRelease(buffer, OFFSET_LISTENER_POS, x);
        FLOAT_VIEW.setRelease(buffer, OFFSET_LISTENER_POS + 4, y);
        FLOAT_VIEW.setRelease(buffer, OFFSET_LISTENER_POS + 8, z);

        FLOAT_VIEW.setRelease(buffer, OFFSET_LISTENER_FWD, fX);
        FLOAT_VIEW.setRelease(buffer, OFFSET_LISTENER_FWD + 4, fY);
        FLOAT_VIEW.setRelease(buffer, OFFSET_LISTENER_FWD + 8, fZ);

        FLOAT_VIEW.setRelease(buffer, OFFSET_LISTENER_UP, uX);
        FLOAT_VIEW.setRelease(buffer, OFFSET_LISTENER_UP + 4, uY);
        FLOAT_VIEW.setRelease(buffer, OFFSET_LISTENER_UP + 8, uZ);

        INT_VIEW.setRelease(buffer, OFFSET_VER, ver + 2);
    }

    public synchronized void updateGlobalState(float[] categoryVols, int flags) {
        int ver = (int) INT_VIEW.getAcquire(buffer, OFFSET_VER);
        INT_VIEW.setRelease(buffer, OFFSET_VER, ver + 1);

        for (int i = 0; i < 16; i++) {
            float vol = (i < categoryVols.length) ? categoryVols[i] : 1.0f;
            FLOAT_VIEW.setRelease(buffer, OFFSET_CATEGORY_VOLUMES + (i * 4), vol);
        }
        INT_VIEW.setRelease(buffer, OFFSET_FLAGS, flags);
        INT_VIEW.setRelease(buffer, OFFSET_VER, ver + 2);
    }

    public synchronized void updateOutputDevice(String deviceName) {
        if (deviceName == null || deviceName.equals(lastSentDevice)) return;
        byte[] bytes = deviceName.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        int writeLen = Math.min(bytes.length, 127);
        for (int i = 0; i < writeLen; i++) buffer.put(OFFSET_DEV_NAME + i, bytes[i]);
        buffer.put(OFFSET_DEV_NAME + writeLen, (byte) 0);
        lastDeviceSeq++;
        INT_VIEW.setRelease(buffer, OFFSET_DEV_SEQ, lastDeviceSeq);
        lastSentDevice = deviceName;
    }

    public boolean writeUpdatePosEvent(int uid, float x, float y, float z) {
        int seq = writeSequence.getAndIncrement();
        int rustReadSeq = (int) INT_VIEW.getAcquire(buffer, OFFSET_RUST_READ_SEQ);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) return false;

        int slotIndex = seq & (QUEUE_CAPACITY - 1);
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_NONE);
        INT_VIEW.setRelease(buffer, offset + 4, uid);
        FLOAT_VIEW.setRelease(buffer, offset + 8, x);
        FLOAT_VIEW.setRelease(buffer, offset + 12, y);
        FLOAT_VIEW.setRelease(buffer, offset + 16, z);

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_UPDATE_POS); // Opcode 3
        INT_VIEW.setRelease(buffer, OFFSET_JAVA_WRITE_SEQ, seq + 1);
        return true;
    }

    public boolean writePlayEvent(int uid, float x, float y, float z, float volume, float pitch, int assetHash, boolean relative, boolean spatial, int categoryId) {
        int seq = writeSequence.getAndIncrement();
        int rustReadSeq = (int) INT_VIEW.getAcquire(buffer, OFFSET_RUST_READ_SEQ);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) return false;

        int slotIndex = seq & (QUEUE_CAPACITY - 1);
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_NONE);
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

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_PLAY);
        INT_VIEW.setRelease(buffer, OFFSET_JAVA_WRITE_SEQ, seq + 1);
        return true;
    }

    public boolean writeStopEvent(int uid) {
        int seq = writeSequence.getAndIncrement();
        int rustReadSeq = (int) INT_VIEW.getAcquire(buffer, OFFSET_RUST_READ_SEQ);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) return false;

        int slotIndex = seq & (QUEUE_CAPACITY - 1);
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_NONE);
        INT_VIEW.setRelease(buffer, offset + 4, uid);

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_STOP);
        INT_VIEW.setRelease(buffer, OFFSET_JAVA_WRITE_SEQ, seq + 1);
        return true;
    }

    public boolean writeStopAllEvent() {
        int seq = writeSequence.getAndIncrement();
        int rustReadSeq = (int) INT_VIEW.getAcquire(buffer, OFFSET_RUST_READ_SEQ);
        if (seq - rustReadSeq >= QUEUE_CAPACITY) return false;

        int slotIndex = seq & (QUEUE_CAPACITY - 1);
        int offset = HEADER_SIZE + (slotIndex * SLOT_SIZE);

        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_NONE);
        INT_VIEW.setRelease(buffer, offset, OpCodes.OP_STOP_ALL);
        INT_VIEW.setRelease(buffer, OFFSET_JAVA_WRITE_SEQ, seq + 1);
        return true;
    }
}