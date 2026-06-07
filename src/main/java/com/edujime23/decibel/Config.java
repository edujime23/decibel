package com.edujime23.decibel;

import net.neoforged.neoforge.common.ModConfigSpec;
import java.util.List;

public class Config {
    public static final ModConfigSpec SPEC;

    public static final ModConfigSpec.BooleanValue LOG_DIRT_BLOCK;
    public static final ModConfigSpec.ConfigValue<String> MAGIC_NUMBER_INTRODUCTION;
    public static final ModConfigSpec.IntValue MAGIC_NUMBER;
    public static final ModConfigSpec.ConfigValue<List<? extends String>> ITEM_STRINGS;

    // Steam Audio Feature Option Toggles
    public static final ModConfigSpec.BooleanValue ENABLE_STEAM_AUDIO;
    public static final ModConfigSpec.BooleanValue ENABLE_OCCLUSION;
    public static final ModConfigSpec.BooleanValue ENABLE_TRANSMISSION;
    public static final ModConfigSpec.BooleanValue ENABLE_REVERB;
    public static final ModConfigSpec.BooleanValue ENABLE_REFLECTION;

    static {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();

        builder.push("general");
        LOG_DIRT_BLOCK = builder.comment("Whether to log dirt block info").define("logDirtBlock", true);
        MAGIC_NUMBER_INTRODUCTION = builder.comment("Magic number introduction").define("magicNumberIntroduction", "The magic number is... ");
        MAGIC_NUMBER = builder.comment("A magic number").defineInRange("magicNumber", 42, 0, 100);
        ITEM_STRINGS = builder.comment("A list of items").defineListAllowEmpty("items", List.of("minecraft:iron_ingot"), (obj) -> obj instanceof String);
        builder.pop();

        builder.push("steam_audio_settings");
        ENABLE_STEAM_AUDIO = builder.comment("Enable or disable dynamic Steam Audio rendering globally.").define("enableSteamAudio", true);
        ENABLE_OCCLUSION = builder.comment("Enable or disable sound occlusion calculations behind solid geometry.").define("enableOcclusion", true);
        ENABLE_TRANSMISSION = builder.comment("Enable or disable sound transmission (sounds passing through solid walls).").define("enableTransmission", true);
        ENABLE_REVERB = builder.comment("Enable or disable physics-based environmental reverberation.").define("enableReverb", true);
        ENABLE_REFLECTION = builder.comment("Enable or disable real-time acoustic reflections/echoes.").define("enableReflection", true);
        builder.pop();

        SPEC = builder.build();
    }
}