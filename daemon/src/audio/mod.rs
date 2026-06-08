pub mod engine;
pub mod source;
pub mod reverb;

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use cpal::traits::{DeviceTrait, HostTrait};

use crate::AppState;
use crate::asset::PCMAsset;
use crate::phonon;

use source::{ActiveSound, DeferredPlay, AudioSource};
use engine::build_stream_result;

pub enum AudioCommand {
    PlaySound {
        uid: u32, pos: [f32; 3], volume: f32, pitch: f32, asset_hash: u32,
        is_relative: bool, is_spatial: bool, category_id: usize,
    },
    PlayStream {
        uid: u32, pos: [f32; 3], volume: f32, pitch: f32,
        is_relative: bool, is_spatial: bool, category_id: usize,
        sample_rate: u32, channels: u16,
    },
    QueueStreamData { uid: u32, samples: Vec<f32> },
    StopSound { uid: u32 },
    StopAllSounds,
    LoadAsset { hash: u32, asset: PCMAsset },
    UpdateListener {
        pos: [f32; 3], fwd: [f32; 3], up: [f32; 3], category_volumes: [f32; 16], engine_flags: u32,
    },
    ChangeDevice { name: String },
}

pub struct SharedAudioState {
    pub active_sounds: Vec<ActiveSound>,
    pub asset_cache: HashMap<u32, PCMAsset>,
    pub deferred_plays: Vec<DeferredPlay>,
    pub stream_senders: HashMap<u32, crossbeam_channel::Sender<Vec<f32>>>,
    pub listener_pos: [f32; 3],
    pub listener_fwd: [f32; 3],
    pub listener_up: [f32; 3],
    pub category_volumes: [f32; 16],
    pub engine_flags: u32,
    pub accum_l: Vec<f32>,
    pub accum_r: Vec<f32>,
    pub resample_cursor: f32,
}

pub fn run_audio_thread(
    device: cpal::Device, config: cpal::SupportedStreamConfig,
    app_state: Arc<AppState>, rx_cmd: crossbeam_channel::Receiver<AudioCommand>,
) {
    let shared_state = Arc::new(Mutex::new(SharedAudioState {
        active_sounds: Vec::new(),
        asset_cache: HashMap::new(),
        deferred_plays: Vec::new(),
        stream_senders: HashMap::new(),
        listener_pos: [0.0; 3], listener_fwd: [0.0; 3], listener_up: [0.0; 3],
        category_volumes: [1.0; 16], engine_flags: 0,
        accum_l: Vec::new(), accum_r: Vec::new(), resample_cursor: 0.0,
    }));

    let mut current_device_name = device.name().ok();
    let mut selected_device_name = "System Default".to_string();

    let state_cb = Arc::clone(&shared_state);
    let app_state_cb = Arc::clone(&app_state);
    let mut stream = engine::build_stream(&device, &config, state_cb, app_state_cb);
    let _ = cpal::traits::StreamTrait::play(&stream);

    loop {
        let result = rx_cmd.recv_timeout(std::time::Duration::from_millis(100));
        match result {
            Ok(cmd) => match cmd {
                AudioCommand::LoadAsset { hash, asset } => {
                    if let Ok(mut state) = shared_state.lock() {
                        state.asset_cache.insert(hash, asset.clone());

                        let mut i = 0;
                        while i < state.deferred_plays.len() {
                            if state.deferred_plays[i].asset_hash == hash {
                                let deferred = state.deferred_plays.remove(i);
                                let base_step = asset.sample_rate as f32 / 48000.0f32;
                                let direct = phonon::SteamDirectEffect::new(app_state.context, 48000, 512);
                                let binaural = phonon::SteamBinauralEffect::new(app_state.context, 48000, 512, app_state.hrtf);

                                state.active_sounds.push(ActiveSound {
                                    uid: deferred.uid,
                                    source: AudioSource::Static { pcm: Arc::clone(&asset.pcm), cursor: 0.0 },
                                    volume: deferred.volume,
                                    pitch_step: base_step * deferred.pitch,
                                    channels: asset.channels,
                                    pos: deferred.pos,
                                    is_relative: deferred.is_relative,
                                    is_spatial: deferred.is_spatial,
                                    category_id: deferred.category_id,
                                    direct_effect: direct,
                                    binaural_effect: binaural,
                                });
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
                AudioCommand::UpdateListener { pos, fwd, up, category_volumes, engine_flags } => {
                    if let Ok(mut state) = shared_state.lock() {
                        state.listener_pos = pos; state.listener_fwd = fwd; state.listener_up = up;
                        state.category_volumes = category_volumes; state.engine_flags = engine_flags;
                    }
                }
                AudioCommand::StopSound { uid } => {
                    if let Ok(mut state) = shared_state.lock() {
                        state.active_sounds.retain(|s| s.uid != uid);
                        state.deferred_plays.retain(|s| s.uid != uid);
                        state.stream_senders.remove(&uid);
                    }
                }
                AudioCommand::StopAllSounds => {
                    if let Ok(mut state) = shared_state.lock() {
                        state.active_sounds.clear(); state.deferred_plays.clear(); state.stream_senders.clear();
                    }
                }
                AudioCommand::PlaySound { uid, pos, volume, pitch, asset_hash, is_relative, is_spatial, category_id } => {
                    let mut pcm_to_add = None;
                    if let Ok(state) = shared_state.lock() {
                        if let Some(cached) = state.asset_cache.get(&asset_hash) {
                            pcm_to_add = Some((Arc::clone(&cached.pcm), cached.sample_rate, cached.channels));
                        }
                    }

                    if let Some((pcm, sample_rate, channels)) = pcm_to_add {
                        let base_step = sample_rate as f32 / 48000.0f32;
                        let direct = phonon::SteamDirectEffect::new(app_state.context, 48000, 512);
                        let binaural = phonon::SteamBinauralEffect::new(app_state.context, 48000, 512, app_state.hrtf);

                        if let Ok(mut state) = shared_state.lock() {
                            state.active_sounds.push(ActiveSound {
                                uid,
                                source: AudioSource::Static { pcm, cursor: 0.0 },
                                volume, pitch_step: base_step * pitch, channels, pos, is_relative, is_spatial, category_id,
                                direct_effect: direct, binaural_effect: binaural,
                            });
                        }
                    } else {
                        if let Ok(mut state) = shared_state.lock() {
                            state.deferred_plays.push(DeferredPlay {
                                uid, pos, volume, pitch, asset_hash, is_relative, is_spatial, category_id,
                            });
                        }
                    }
                }
                AudioCommand::PlayStream { uid, pos, volume, pitch, is_relative, is_spatial, category_id, sample_rate, channels } => {
                    let (tx, rx) = crossbeam_channel::unbounded::<Vec<f32>>();
                    let direct = phonon::SteamDirectEffect::new(app_state.context, 48000, 512);
                    let binaural = phonon::SteamBinauralEffect::new(app_state.context, 48000, 512, app_state.hrtf);

                    let base_step = sample_rate as f32 / 48000.0f32;

                    if let Ok(mut state) = shared_state.lock() {
                        state.stream_senders.insert(uid, tx);
                        state.active_sounds.push(ActiveSound {
                            uid,
                            source: AudioSource::Stream { rx, buffer: Vec::new(), cursor: 0.0 },
                            volume, pitch_step: pitch * base_step, channels, pos, is_relative, is_spatial, category_id,
                            direct_effect: direct, binaural_effect: binaural,
                        });
                    }
                }
                AudioCommand::QueueStreamData { uid, samples } => {
                    if let Ok(state) = shared_state.lock() {
                        if let Some(tx) = state.stream_senders.get(&uid) {
                            let _ = tx.send(samples);
                        }
                    }
                }
                AudioCommand::ChangeDevice { name } => {
                    selected_device_name = name.clone();
                    let host = cpal::default_host();
                    let found_device = if name.is_empty() || name == "System Default" {
                        host.default_output_device()
                    } else {
                        host.output_devices().ok().and_then(|mut devices| {
                            devices.find(|d| d.name().map_or(false, |n| {
                                let cp_name = n.to_lowercase();
                                let mc_name = name.to_lowercase();
                                cp_name.contains(&mc_name) || mc_name.contains(&cp_name)
                            }))
                        })
                    };

                    if let Some(new_dev) = found_device {
                        if let Ok(new_cfg) = new_dev.default_output_config() {
                            drop(stream);
                            let state_cb = Arc::clone(&shared_state);
                            let app_state_cb = Arc::clone(&app_state);
                            match build_stream_result(&new_dev, &new_cfg, state_cb, app_state_cb) {
                                Ok(new_stream) => {
                                    stream = new_stream;
                                    let _ = cpal::traits::StreamTrait::play(&stream);
                                    if let Ok(mut state) = shared_state.lock() {
                                        state.accum_l.clear(); state.accum_r.clear(); state.resample_cursor = 0.0;
                                    }
                                    current_device_name = new_dev.name().ok();
                                }
                                Err(_) => {
                                    let state_cb = Arc::clone(&shared_state);
                                    let app_state_cb = Arc::clone(&app_state);
                                    stream = engine::build_stream(&device, &config, state_cb, app_state_cb);
                                    let _ = cpal::traits::StreamTrait::play(&stream);
                                }
                            }
                        }
                    }
                }
            },
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if selected_device_name.is_empty() || selected_device_name == "System Default" {
                    let host = cpal::default_host();
                    if let Some(default_dev) = host.default_output_device() {
                        if let Ok(default_name) = default_dev.name() {
                            let should_migrate = current_device_name.as_ref() != Some(&default_name);
                            if should_migrate {
                                if let Ok(new_cfg) = default_dev.default_output_config() {
                                    drop(stream);
                                    let state_cb = Arc::clone(&shared_state);
                                    let app_state_cb = Arc::clone(&app_state);
                                    if let Ok(new_stream) = build_stream_result(&default_dev, &new_cfg, state_cb, app_state_cb) {
                                        stream = new_stream;
                                        let _ = cpal::traits::StreamTrait::play(&stream);
                                        if let Ok(mut state) = shared_state.lock() {
                                            state.accum_l.clear(); state.accum_r.clear(); state.resample_cursor = 0.0;
                                        }
                                        current_device_name = Some(default_name);
                                    } else {
                                        let state_cb = Arc::clone(&shared_state);
                                        let app_state_cb = Arc::clone(&app_state);
                                        stream = engine::build_stream(&device, &config, state_cb, app_state_cb);
                                        let _ = cpal::traits::StreamTrait::play(&stream);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}