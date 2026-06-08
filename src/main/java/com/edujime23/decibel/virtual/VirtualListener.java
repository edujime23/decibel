package com.edujime23.decibel.virtual;

import com.edujime23.decibel.daemon.DaemonManager;

public class VirtualListener {
    public static void reset() {
        if (DaemonManager.ipc != null) {
            DaemonManager.ipc.updateListener(
                0.0f, 0.0f, 0.0f,
                0.0f, 0.0f, -1.0f,
                0.0f, 1.0f, 0.0f
            );
        }
    }

    public static void setListenerPosition(float x, float y, float z) {
        if (DaemonManager.ipc != null) {
            DaemonManager.ipc.updateListener(
                x, y, z,
                0.0f, 0.0f, -1.0f,
                0.0f, 1.0f, 0.0f
            );
        }
    }

    public static void setListenerOrientation(float fwdX, float fwdY, float fwdZ, float upX, float upY, float upZ) {
        if (DaemonManager.ipc != null) {
            DaemonManager.ipc.updateListener(
                0.0f, 0.0f, 0.0f,
                fwdX, fwdY, fwdZ,
                upX, upY, upZ
            );
        }
    }

    public static void setGain(float gain) {}
}