package com.edujime23.decibel;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.reflect.TypeToken;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.File;
import java.io.FileReader;
import java.io.FileWriter;
import java.lang.reflect.Type;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.HashMap;
import java.util.Map;

public class MaterialRegistry {
    private static final Logger LOGGER = LoggerFactory.getLogger("Decibel-Materials");
    private static final Map<String, Byte> MATERIAL_MAPPINGS = new HashMap<>();
    private static final Gson GSON = new GsonBuilder().setPrettyPrinting().create();

    public static void init() {
        try {
            Path configDir = Paths.get("config", "decibel");
            Files.createDirectories(configDir);
            File configFile = configDir.resolve("materials.json").toFile();

            if (!configFile.exists()) {
                LOGGER.info("Generating default acoustic materials JSON configuration...");
                Map<String, Integer> defaultMap = createDefaultMap();
                try (FileWriter writer = new FileWriter(configFile)) {
                    GSON.toJson(defaultMap, writer);
                }
            }

            LOGGER.info("Loading block acoustic material configurations...");
            Type type = new TypeToken<Map<String, Byte>>(){}.getType();
            try (FileReader reader = new FileReader(configFile)) {
                Map<String, Byte> loaded = GSON.fromJson(reader, type);
                if (loaded != null) {
                    MATERIAL_MAPPINGS.clear();
                    loaded.forEach((key, val) -> MATERIAL_MAPPINGS.put(key.toLowerCase(), val));
                }
            }
            LOGGER.info("Successfully loaded {} custom block acoustic material definitions.", MATERIAL_MAPPINGS.size());

        } catch (Exception e) {
            LOGGER.error("Failed to load block material configuration! Falling back to stone default.", e);
        }
    }

    public static byte getMaterialId(String blockId) {
        return MATERIAL_MAPPINGS.getOrDefault(blockId.toLowerCase(), (byte) 1); // Defaults to STONE (1) [7.2]
    }

    private static Map<String, Integer> createDefaultMap() {
        Map<String, Integer> defaults = new HashMap<>();

        // Refective Profiles: STONE/METAL -> 1 [7.2]
        defaults.put("minecraft:stone", 1);
        defaults.put("minecraft:cobblestone", 1);
        defaults.put("minecraft:granite", 1);
        defaults.put("minecraft:diorite", 1);
        defaults.put("minecraft:andesite", 1);
        defaults.put("minecraft:deepslate", 1);
        defaults.put("minecraft:iron_block", 1);
        defaults.put("minecraft:gold_block", 1);
        defaults.put("minecraft:diamond_block", 1);
        defaults.put("minecraft:bricks", 1);
        defaults.put("minecraft:obsidian", 1);

        // Moderate Profiles: WOOD/PLANKS -> 2 [7.2]
        defaults.put("minecraft:oak_planks", 2);
        defaults.put("minecraft:spruce_planks", 2);
        defaults.put("minecraft:birch_planks", 2);
        defaults.put("minecraft:jungle_planks", 2);
        defaults.put("minecraft:acacia_planks", 2);
        defaults.put("minecraft:dark_oak_planks", 2);
        defaults.put("minecraft:mangrove_planks", 2);
        defaults.put("minecraft:cherry_planks", 2);
        defaults.put("minecraft:bamboo_planks", 2);
        defaults.put("minecraft:oak_log", 2);
        defaults.put("minecraft:spruce_log", 2);

        // Absorptive Profiles: WOOL/LEAVES/CARPET -> 3 [7.2]
        defaults.put("minecraft:oak_leaves", 3);
        defaults.put("minecraft:spruce_leaves", 3);
        defaults.put("minecraft:birch_leaves", 3);
        defaults.put("minecraft:white_wool", 3);
        defaults.put("minecraft:orange_wool", 3);
        defaults.put("minecraft:magenta_wool", 3);
        defaults.put("minecraft:light_blue_wool", 3);
        defaults.put("minecraft:yellow_wool", 3);
        defaults.put("minecraft:lime_wool", 3);
        defaults.put("minecraft:pink_wool", 3);
        defaults.put("minecraft:gray_wool", 3);
        defaults.put("minecraft:light_gray_wool", 3);
        defaults.put("minecraft:cyan_wool", 3);
        defaults.put("minecraft:purple_wool", 3);
        defaults.put("minecraft:blue_wool", 3);
        defaults.put("minecraft:brown_wool", 3);
        defaults.put("minecraft:green_wool", 3);
        defaults.put("minecraft:red_wool", 3);
        defaults.put("minecraft:black_wool", 3);

        // Transparent/Refractive Profiles: GLASS -> 4 [7.2]
        defaults.put("minecraft:glass", 4);
        defaults.put("minecraft:white_stained_glass", 4);
        defaults.put("minecraft:orange_stained_glass", 4);
        defaults.put("minecraft:magenta_stained_glass", 4);
        defaults.put("minecraft:light_blue_stained_glass", 4);
        defaults.put("minecraft:yellow_stained_glass", 4);
        defaults.put("minecraft:lime_stained_glass", 4);
        defaults.put("minecraft:pink_stained_glass", 4);
        defaults.put("minecraft:gray_stained_glass", 4);
        defaults.put("minecraft:light_gray_stained_glass", 4);
        defaults.put("minecraft:cyan_stained_glass", 4);
        defaults.put("minecraft:purple_stained_glass", 4);
        defaults.put("minecraft:blue_stained_glass", 4);
        defaults.put("minecraft:brown_stained_glass", 4);
        defaults.put("minecraft:green_stained_glass", 4);
        defaults.put("minecraft:red_stained_glass", 4);
        defaults.put("minecraft:black_stained_glass", 4);

        return defaults;
    }
}