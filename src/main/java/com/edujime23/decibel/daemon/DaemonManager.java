package com.edujime23.decibel.daemon;

import com.edujime23.decibel.AssetCacher;
import com.edujime23.decibel.Decibel;
import com.edujime23.decibel.MaterialRegistry;
import com.edujime23.decibel.ipc.DaemonChannel;
import com.edujime23.decibel.ipc.SharedMemoryRingBuffer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.File;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
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

            Path tmpDir = Paths.get(System.getProperty("java.io.tmpdir"), "decibel_engine");
            Files.createDirectories(tmpDir);

            File shmFile = tmpDir.resolve("decibel_shm.dat").toFile();
            ipc = new SharedMemoryRingBuffer(shmFile);
            LOGGER.info("Shared Memory Ring Buffer mapped at: {}", shmFile.getAbsolutePath());

            channel = new DaemonChannel(tmpDir, isWindows);
            AssetCacher.init(channel);

            String daemonName = isWindows ? "daemon.exe" : "daemon";
            Path daemonPath = tmpDir.resolve(daemonName);
            extractResource("/bin/daemon/" + daemonName, daemonPath);
            daemonPath.toFile().setExecutable(true, true);

            if (isWindows) {
                extractResource("/bin/steamaudio/windows/x64/phonon.dll", tmpDir.resolve("phonon.dll"));
                extractResource("/bin/steamaudio/windows/x64/TrueAudioNext.dll", tmpDir.resolve("TrueAudioNext.dll"));
                extractResource("/bin/steamaudio/windows/x64/GPUUtilities.dll", tmpDir.resolve("GPUUtilities.dll"));
            } else if (isMac) {
                extractResource("/bin/steamaudio/macos/libphonon.dylib", tmpDir.resolve("libphonon.dylib"));
            } else {
                extractResource("/bin/steamaudio/linux/x64/libphonon.so", tmpDir.resolve("libphonon.so"));
            }

            ProcessBuilder pb = new ProcessBuilder(daemonPath.toAbsolutePath().toString());
            pb.directory(tmpDir.toFile());

            pb.environment().put("DECIBEL_SHM_PATH", shmFile.getAbsolutePath());
            pb.environment().put(isWindows ? "PATH" : "LD_LIBRARY_PATH", tmpDir.toAbsolutePath().toString());

            pb.redirectOutput(ProcessBuilder.Redirect.INHERIT);
            pb.redirectError(ProcessBuilder.Redirect.INHERIT);

            daemonProcess = pb.start();
            channel.connect();

            Runtime.getRuntime().addShutdownHook(new Thread(() -> {
                if (daemonProcess != null && daemonProcess.isAlive()) {
                    LOGGER.info("JVM shutting down. Terminating Rust Daemon...");
                    daemonProcess.destroyForcibly();
                }
                if (channel != null) {
                    channel.close();
                }
            }));

            DaemonWatchdog.start(daemonProcess);

        } catch (Exception e) {
            LOGGER.error("CRITICAL FAILURE: Could not boot Steam Audio Daemon!", e);
        }
    }

    private static void extractResource(String resourcePath, Path targetPath) throws Exception {
        try (InputStream is = DaemonManager.class.getResourceAsStream(resourcePath)) {
            if (is == null) {
                LOGGER.warn("Could not find resource inside JAR: " + resourcePath);
                return;
            }
            Files.copy(is, targetPath, StandardCopyOption.REPLACE_EXISTING);
        }
    }
}