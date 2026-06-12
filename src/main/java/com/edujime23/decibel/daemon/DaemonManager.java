package com.edujime23.decibel.daemon;

import com.edujime23.decibel.AssetCacher;
import com.edujime23.decibel.MaterialRegistry;
import com.edujime23.decibel.ipc.DaemonChannel;
import com.edujime23.decibel.ipc.SharedMemoryRingBuffer;
import net.neoforged.fml.loading.FMLPaths;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.File;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Locale;

public class DaemonManager {
    private static final Logger LOGGER = LoggerFactory.getLogger("Decibel-Daemon");
    private static Process daemonProcess;
    public static SharedMemoryRingBuffer ipc;
    public static DaemonChannel channel;

    public static void init() {
        try {
            LOGGER.info("Initializing Decibel Out-of-Process Audio Engine...");
            MaterialRegistry.init();

            String osName = System.getProperty("os.name").toLowerCase(Locale.ROOT);
            boolean isWindows = osName.contains("win");
            boolean isMac = osName.contains("mac");

            Path nativeDir = FMLPaths.GAMEDIR.get().resolve("decibel_natives");
            Files.createDirectories(nativeDir);

            File shmFile = nativeDir.resolve("decibel_shm.dat").toFile();
            ipc = new SharedMemoryRingBuffer(shmFile);
            channel = new DaemonChannel(nativeDir, isWindows);
            AssetCacher.init(channel);

            String daemonName = isWindows ? "daemon.exe" : "daemon";
            Path daemonPath = nativeDir.resolve(daemonName);
            extractResource("/bin/daemon/" + daemonName, daemonPath);
            daemonPath.toFile().setExecutable(true, true);

            if (isWindows) {
                extractResource("/bin/steamaudio/windows/x64/phonon.dll", nativeDir.resolve("phonon.dll"));
            } else if (isMac) {
                extractResource("/bin/steamaudio/macos/libphonon.dylib", nativeDir.resolve("libphonon.dylib"));
            } else {
                extractResource("/bin/steamaudio/linux/x64/libphonon.so", nativeDir.resolve("libphonon.so"));
            }

            ProcessBuilder pb = new ProcessBuilder(daemonPath.toAbsolutePath().toString());
            pb.directory(nativeDir.toFile());
            pb.environment().put("DECIBEL_SHM_PATH", shmFile.getAbsolutePath());
            String currentPath = pb.environment().getOrDefault("PATH", "");
            pb.environment().put("PATH", nativeDir.toAbsolutePath().toString() + java.io.File.pathSeparator + currentPath);
            pb.redirectOutput(ProcessBuilder.Redirect.INHERIT);
            pb.redirectError(ProcessBuilder.Redirect.INHERIT);

            daemonProcess = pb.start();
            channel.connect();
            DaemonWatchdog.start(daemonProcess);

            // Flawless Dead-Man's Switch: Background Thread totally immune to Minecraft Loading Lag
            Thread heartbeatThread = new Thread(() -> {
                while (true) {
                    try {
                        Thread.sleep(1000); // 1 tick per second
                        if (ipc != null) ipc.writeHeartbeat();
                    } catch (InterruptedException e) { break; }
                }
            });
            heartbeatThread.setDaemon(true); // Dies automatically when the JVM terminates
            heartbeatThread.setName("Decibel-Heartbeat");
            heartbeatThread.start();

        } catch (Exception e) {
            LOGGER.error("CRITICAL FAILURE: Could not boot Steam Audio Daemon!", e);
        }
    }

    private static void extractResource(String resourcePath, Path targetPath) throws Exception {
        try (InputStream is = DaemonManager.class.getResourceAsStream(resourcePath)) {
            if (is == null) return;
            Files.copy(is, targetPath, StandardCopyOption.REPLACE_EXISTING);
        }
    }
}