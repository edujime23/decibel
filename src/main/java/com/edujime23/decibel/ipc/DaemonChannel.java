package com.edujime23.decibel.ipc;

import com.edujime23.decibel.Decibel;
import java.io.RandomAccessFile;
import java.net.StandardProtocolFamily;
import java.net.UnixDomainSocketAddress;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.channels.SocketChannel;
import java.nio.file.Path;

public class DaemonChannel {
    private final boolean isWindows;
    private final Path socketPath;
    private final String pipePath = "\\\\.\\pipe\\decibel_engine";

    private RandomAccessFile windowsPipe;
    private SocketChannel unixSocket;

    public DaemonChannel(Path tmpDir, boolean isWindows) {
        this.isWindows = isWindows;
        this.socketPath = tmpDir.resolve("decibel_engine.sock");
    }

    public void connect() throws Exception {
        if (isWindows) {
            int attempts = 0;
            while (attempts < 20) {
                try {
                    this.windowsPipe = new RandomAccessFile(pipePath, "rw");
                    break;
                } catch (Exception e) {
                    attempts++;
                    Thread.sleep(100);
                }
            }
            if (windowsPipe == null) {
                throw new IllegalStateException("Failed to connect to Windows Named Pipe: " + pipePath);
            }
        } else {
            UnixDomainSocketAddress address = UnixDomainSocketAddress.of(socketPath.toAbsolutePath().toString());
            int attempts = 0;
            while (attempts < 20) {
                try {
                    this.unixSocket = SocketChannel.open(StandardProtocolFamily.UNIX);
                    this.unixSocket.connect(address);
                    break;
                } catch (Exception e) {
                    attempts++;
                    Thread.sleep(100);
                }
            }
            if (unixSocket == null || !unixSocket.isConnected()) {
                throw new IllegalStateException("Failed to connect to Unix Socket: " + socketPath);
            }
        }
        Decibel.LOGGER.info("Successfully connected to Native Signal Channel.");
    }

    public synchronized void sendAsset(int assetHash, byte[] rawOggBytes) {
        try {
            ByteBuffer header = ByteBuffer.allocate(13);
            header.order(ByteOrder.LITTLE_ENDIAN);
            header.put("DCBL".getBytes());
            header.put((byte) 0x01);
            header.putInt(assetHash);
            header.putInt(rawOggBytes.length);

            if (isWindows) {
                windowsPipe.write(header.array());
                windowsPipe.write(rawOggBytes);
            } else {
                unixSocket.write(ByteBuffer.wrap(header.array()));
                unixSocket.write(ByteBuffer.wrap(rawOggBytes));
            }
        } catch (Exception e) {
            Decibel.LOGGER.error("Failed to send asset over Native Signal Channel", e);
        }
    }

    public synchronized void sendPlayStream(int uid, float x, float y, float z, float volume, float pitch, boolean relative, boolean spatial, int categoryId, int sampleRate, int channels) {
        try {
            int payloadSize = 28;
            ByteBuffer packet = ByteBuffer.allocate(13 + payloadSize);
            packet.order(ByteOrder.LITTLE_ENDIAN);
            packet.put("DCBL".getBytes());
            packet.put((byte) 0x02); // OP_PLAY_STREAM
            packet.putInt(uid);
            packet.putInt(payloadSize);

            packet.putFloat(x);
            packet.putFloat(y);
            packet.putFloat(z);
            packet.putFloat(volume);
            packet.putFloat(pitch);
            packet.put((byte) (relative ? 1 : 0));
            packet.put((byte) (spatial ? 1 : 0));
            packet.put((byte) categoryId);

            packet.putInt(sampleRate);
            packet.put((byte) channels);

            if (isWindows) {
                windowsPipe.write(packet.array());
            } else {
                unixSocket.write(ByteBuffer.wrap(packet.array()));
            }
        } catch (Exception e) {
            Decibel.LOGGER.error("Failed to send play stream command over Native Signal Channel", e);
        }
    }

    public synchronized void sendStreamData(int uid, float[] samples) {
        try {
            int payloadSize = samples.length * 4;
            ByteBuffer packet = ByteBuffer.allocate(13 + payloadSize);
            packet.order(ByteOrder.LITTLE_ENDIAN);
            packet.put("DCBL".getBytes());
            packet.put((byte) 0x03); // OP_STREAM_DATA
            packet.putInt(uid);
            packet.putInt(payloadSize);

            for (float sample : samples) {
                packet.putFloat(sample);
            }

            if (isWindows) {
                windowsPipe.write(packet.array());
            } else {
                unixSocket.write(ByteBuffer.wrap(packet.array()));
            }
        } catch (Exception e) {
            Decibel.LOGGER.error("Failed to send stream data packet over Native Signal Channel", e);
        }
    }

    public void close() {
        try {
            if (windowsPipe != null) windowsPipe.close();
            if (unixSocket != null) unixSocket.close();
        } catch (Exception e) {
            Decibel.LOGGER.error("Error closing Native Signal Channel", e);
        }
    }
}