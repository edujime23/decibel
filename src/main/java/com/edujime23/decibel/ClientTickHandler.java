package com.edujime23.decibel;

import com.edujime23.decibel.asm.SoundInterceptor;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;

public class ClientTickHandler {
    @SubscribeEvent
    public static void onClientTick(ClientTickEvent.Post event) {
        SoundInterceptor.syncGlobalState();
    }
}