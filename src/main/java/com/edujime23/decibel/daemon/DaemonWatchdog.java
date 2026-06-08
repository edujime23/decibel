package com.edujime23.decibel.daemon;

import com.edujime23.decibel.Decibel;

public class DaemonWatchdog {
    public static void start(Process daemonProcess) {
        Thread watchdog = new Thread(() -> {
            try {
                int exitCode = daemonProcess.waitFor();
                Decibel.LOGGER.error("DAEMON DIED with exit code {}. Audio suspended.", exitCode);
                // Backoff reboot logic fits naturally here in the future
            } catch (InterruptedException e) {
                Decibel.LOGGER.warn("Watchdog thread interrupted.");
            }
        });
        watchdog.setDaemon(true);
        watchdog.setName("Decibel-Watchdog");
        watchdog.start();
    }
}