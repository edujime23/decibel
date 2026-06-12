pub mod engine;
pub mod source;
pub mod reverb;

use crate::asset::PCMAsset;
use crate::phonon::{SteamDirectEffect, SteamBinauralEffect, SteamSource};

pub enum AudioCommand {
    PlaySound {
        uid: u32, pos: [f32; 3], volume: f32, pitch: f32, asset_hash: u32,
        is_relative: bool, is_spatial: bool, category_id: usize,
        direct_effect: Option<SteamDirectEffect>, binaural_effect: Option<SteamBinauralEffect>, ipl_source: Option<SteamSource>,
    },
    PlayStream {
        uid: u32, pos: [f32; 3], volume: f32, pitch: f32,
        is_relative: bool, is_spatial: bool, category_id: usize,
        sample_rate: u32, channels: u16,
        direct_effect: Option<SteamDirectEffect>, binaural_effect: Option<SteamBinauralEffect>, ipl_source: Option<SteamSource>,
    },
    UpdateSoundPosition { uid: u32, pos: [f32; 3] }, // Added
    QueueStreamData { uid: u32, samples: Vec<f32> },
    StopSound { uid: u32 },
    StopAllSounds,
    LoadAsset { hash: u32, asset: PCMAsset },
    UpdateListener {
        pos: [f32; 3], fwd: [f32; 3], up: [f32; 3], category_volumes: [f32; 16], engine_flags: u32,
    },
    ChangeDevice { name: String },
}