use std::sync::Arc;
use crate::phonon;

pub enum AudioSource {
    Static {
        pcm: Arc<Vec<f32>>,
        cursor: f32,
    },
    Stream {
        rx: crossbeam_channel::Receiver<Vec<f32>>,
        buffer: Vec<f32>,
        cursor: f32,
    },
}

pub struct ActiveSound {
    pub uid: u32,
    pub source: AudioSource,
    pub volume: f32,
    pub pitch_step: f32,
    pub channels: u16,
    pub pos: [f32; 3],
    pub is_relative: bool,
    pub is_spatial: bool,
    pub category_id: usize,
    pub direct_effect: Option<phonon::SteamDirectEffect>,
    pub binaural_effect: Option<phonon::SteamBinauralEffect>,
    pub ipl_source: Option<phonon::SteamSource>,
}