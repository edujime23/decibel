package com.edujime23.decibel.virtual;

import com.edujime23.decibel.Decibel;
import com.mojang.blaze3d.audio.Library;

public class VirtualLibrary {
    public static void init(Library library, String device, boolean useNoAudio) {
        Decibel.LOGGER.info("Virtual Sound Library initialized (OpenAL bypassed).");

        try {
            // Instantiate normal CountingChannelPools so Minecraft's ChannelAccess has valid pools to acquire handles from.
            Class<?> poolClass = Class.forName("com.mojang.blaze3d.audio.Library$CountingChannelPool");
            java.lang.reflect.Constructor<?> ctor = poolClass.getDeclaredConstructor(int.class);
            ctor.setAccessible(true);

            Object staticPool = ctor.newInstance(30); // Standard static channel limit
            Object streamingPool = ctor.newInstance(8); // Standard streaming channel limit

            // Locate fields dynamically by searching for the Library$ChannelPool interface.
            Class<?> channelPoolInterface = Class.forName("com.mojang.blaze3d.audio.Library$ChannelPool");

            java.lang.reflect.Field staticField = null;
            java.lang.reflect.Field streamingField = null;

            for (java.lang.reflect.Field f : Library.class.getDeclaredFields()) {
                // Skip helper constants (like Library.EMPTY) which are static final
                if (java.lang.reflect.Modifier.isStatic(f.getModifiers())) {
                    continue;
                }

                if (channelPoolInterface.isAssignableFrom(f.getType())) {
                    f.setAccessible(true);
                    if (staticField == null) {
                        staticField = f;
                    } else if (streamingField == null) {
                        streamingField = f;
                    }
                }
            }

            if (staticField != null && streamingField != null) {
                staticField.set(library, staticPool);
                streamingField.set(library, streamingPool);
                Decibel.LOGGER.info("Successfully populated Virtual Channel Pools inside Library fields: "
                    + staticField.getName() + ", " + streamingField.getName());
            } else {
                Decibel.LOGGER.error("Failed to find Virtual Channel Pool fields in Library class.");
            }

        } catch (Exception e) {
            Decibel.LOGGER.error("Failed to inject virtual channel pools into Library fields", e);
        }
    }

    public static void cleanup() {
        Decibel.LOGGER.info("Virtual Sound Library cleaned up.");
    }
}