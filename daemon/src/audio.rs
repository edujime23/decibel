use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use cpal::traits::{DeviceTrait, StreamTrait, HostTrait};

use crate::AppState;
use crate::asset::PCMAsset;
use crate::steam;

pub enum AudioCommand {
    PlaySound {
        uid: u32,
        pos: [f32; 3],
        volume: f32,
        pitch: f32,
        asset_hash: u32,
        is_relative: bool,
        is_spatial: bool,
        category_id: usize,
    },
    StopSound {
        uid: u32,
    },
    StopAllSounds,
    LoadAsset {
        hash: u32,
        asset: PCMAsset,
    },
    UpdateListener {
        pos: [f32; 3],
        fwd: [f32; 3],
        up: [f32; 3],
        category_volumes: [f32; 16],
        engine_flags: u32,
    },
    ChangeDevice {
        name: String,
    },
}

struct ActiveSound {
    uid: u32,
    pcm: Arc<Vec<f32>>,
    cursor: f32,
    volume: f32,
    pitch_step: f32,
    channels: u16,
    pos: [f32; 3],
    is_relative: bool,
    is_spatial: bool,
    category_id: usize,
    direct_effect: steam::SteamDirectEffect,
    binaural_effect: steam::SteamBinauralEffect,
}

struct DeferredPlay {
    uid: u32,
    pos: [f32; 3],
    volume: f32,
    pitch: f32,
    asset_hash: u32,
    is_relative: bool,
    is_spatial: bool,
    category_id: usize,
}

struct SharedAudioState {
    active_sounds: Vec<ActiveSound>,
    asset_cache: HashMap<u32, PCMAsset>,
    deferred_plays: Vec<DeferredPlay>,
    listener_pos: [f32; 3],
    listener_fwd: [f32; 3],
    listener_up: [f32; 3],
    category_volumes: [f32; 16],
    engine_flags: u32,
    accum_l: Vec<f32>,
    accum_r: Vec<f32>,
    resample_cursor: f32,
}

pub fn run_audio_thread(
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
    app_state: Arc<AppState>,
    rx_cmd: crossbeam_channel::Receiver<AudioCommand>,
) {
    let shared_state = Arc::new(Mutex::new(SharedAudioState {
        active_sounds: Vec::new(),
        asset_cache: HashMap::new(),
        deferred_plays: Vec::new(),
        listener_pos: [0.0; 3],
        listener_fwd: [0.0; 3],
        listener_up: [0.0; 3],
        category_volumes: [1.0; 16],
        engine_flags: 0,
        accum_l: Vec::new(),
        accum_r: Vec::new(),
        resample_cursor: 0.0,
    }));

    let mut current_device_name = device.name().ok();
    let mut selected_device_name = "System Default".to_string();

    let state_cb = Arc::clone(&shared_state);
    let app_state_cb = Arc::clone(&app_state);
    let mut stream = build_stream(&device, &config, state_cb, app_state_cb);
    let _ = stream.play();

    loop {
        // Query interval set to 100ms to achieve instant cross-platform hardware hot-swaps
        let result = rx_cmd.recv_timeout(std::time::Duration::from_millis(100));
        match result {
            Ok(cmd) => {
                match cmd {
                    AudioCommand::LoadAsset { hash, asset } => {
                        if let Ok(mut state) = shared_state.lock() {
                            state.asset_cache.insert(hash, asset.clone());

                            let mut i = 0;
                            while i < state.deferred_plays.len() {
                                if state.deferred_plays[i].asset_hash == hash {
                                    let deferred = state.deferred_plays.remove(i);
                                    let base_step = asset.sample_rate as f32 / 48000.0f32;

                                    let direct = steam::SteamDirectEffect::new(app_state.context, 48000, 512);
                                    let binaural = steam::SteamBinauralEffect::new(
                                        app_state.context,
                                        48000,
                                        512,
                                        app_state.hrtf
                                    );

                                    state.active_sounds.push(ActiveSound {
                                        uid: deferred.uid,
                                        pcm: Arc::clone(&asset.pcm),
                                        cursor: 0.0,
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
                            state.listener_pos = pos;
                            state.listener_fwd = fwd;
                            state.listener_up = up;
                            state.category_volumes = category_volumes;
                            state.engine_flags = engine_flags;
                        }
                    }
                    AudioCommand::StopSound { uid } => {
                        if let Ok(mut state) = shared_state.lock() {
                            state.active_sounds.retain(|s| s.uid != uid);
                            state.deferred_plays.retain(|s| s.uid != uid);
                        }
                    }
                    AudioCommand::StopAllSounds => {
                        if let Ok(mut state) = shared_state.lock() {
                            state.active_sounds.clear();
                            state.deferred_plays.clear();
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
                            let direct = steam::SteamDirectEffect::new(app_state.context, 48000, 512);
                            let binaural = steam::SteamBinauralEffect::new(
                                app_state.context,
                                48000,
                                512,
                                app_state.hrtf
                            );

                            if let Ok(mut state) = shared_state.lock() {
                                state.active_sounds.push(ActiveSound {
                                    uid,
                                    pcm,
                                    cursor: 0.0,
                                    volume,
                                    pitch_step: base_step * pitch,
                                    channels,
                                    pos,
                                    is_relative,
                                    is_spatial,
                                    category_id,
                                    direct_effect: direct,
                                    binaural_effect: binaural,
                                });
                            }
                        } else {
                            if let Ok(mut state) = shared_state.lock() {
                                state.deferred_plays.push(DeferredPlay {
                                    uid,
                                    pos,
                                    volume,
                                    pitch,
                                    asset_hash,
                                    is_relative,
                                    is_spatial,
                                    category_id,
                                });
                                println!("[Rust Daemon] Deferred play command cached for asset: {}", asset_hash);
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
                                devices.find(|d| {
                                    d.name().map_or(false, |n| {
                                        let cp_name = n.to_lowercase();
                                        let mc_name = name.to_lowercase();
                                        cp_name.contains(&mc_name) || mc_name.contains(&cp_name)
                                    })
                                })
                            })
                        };

                        if let Some(new_dev) = found_device {
                            if let Ok(new_cfg) = new_dev.default_output_config() {
                                let _ = stream.pause();
                                drop(stream); // Safely unbind from the OS audio driver first

                                let state_cb = Arc::clone(&shared_state);
                                let app_state_cb = Arc::clone(&app_state);
                                match build_stream_result(&new_dev, &new_cfg, state_cb, app_state_cb) {
                                    Ok(new_stream) => {
                                        stream = new_stream;
                                        let _ = stream.play();

                                        // Reset resampler cache states on hardware migration
                                        if let Ok(mut state) = shared_state.lock() {
                                            state.accum_l.clear();
                                            state.accum_r.clear();
                                            state.resample_cursor = 0.0;
                                        }

                                        current_device_name = new_dev.name().ok();
                                        println!("[Rust Daemon] Output swapped to: {:?}", current_device_name);
                                    }
                                    Err(_) => {
                                        let state_cb = Arc::clone(&shared_state);
                                        let app_state_cb = Arc::clone(&app_state);
                                        stream = build_stream(&device, &config, state_cb, app_state_cb);
                                        let _ = stream.play();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // Dynamic hot-swap fallback check when set to "System Default"
                if selected_device_name.is_empty() || selected_device_name == "System Default" {
                    let host = cpal::default_host();
                    if let Some(default_dev) = host.default_output_device() {
                        if let Ok(default_name) = default_dev.name() {
                            let mut should_migrate = false;
                            if let Some(ref cur_name) = current_device_name {
                                if *cur_name != default_name {
                                    should_migrate = true;
                                }
                            } else {
                                should_migrate = true;
                            }

                            if should_migrate {
                                if let Ok(new_cfg) = default_dev.default_output_config() {
                                    let _ = stream.pause();
                                    drop(stream); // Instantly drop dead handle to unlock the hardware device

                                    let state_cb = Arc::clone(&shared_state);
                                    let app_state_cb = Arc::clone(&app_state);
                                    if let Ok(new_stream) = build_stream_result(&default_dev, &new_cfg, state_cb, app_state_cb) {
                                        stream = new_stream;
                                        let _ = stream.play();

                                        if let Ok(mut state) = shared_state.lock() {
                                            state.accum_l.clear();
                                            state.accum_r.clear();
                                            state.resample_cursor = 0.0;
                                        }

                                        current_device_name = Some(default_name.clone());
                                        println!("[Rust Daemon] System default swapped. Migrated to: {:?}", current_device_name);
                                    } else {
                                        let state_cb = Arc::clone(&shared_state);
                                        let app_state_cb = Arc::clone(&app_state);
                                        stream = build_stream(&device, &config, state_cb, app_state_cb);
                                        let _ = stream.play();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    state: Arc<Mutex<SharedAudioState>>,
    app_state: Arc<AppState>,
) -> cpal::Stream {
    build_stream_result(device, config, state, app_state).expect("Failed to initialize CPAL Stream context")
}

fn build_stream_result(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    state: Arc<Mutex<SharedAudioState>>,
    app_state: Arc<AppState>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let device_sample_rate = config.sample_rate().0 as f32;
    let output_channels = config.channels() as usize;

    device.build_output_stream(
        &config.clone().into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            data.fill(0.0);

            if output_channels != 2 {
                return;
            }

            let mut state = match state.lock() {
                Ok(s) => s,
                Err(_) => return,
            };

            let output_frames = data.len() / 2;
            let ratio = 48000.0f32 / device_sample_rate;
            let needed_samples = (output_frames as f32 * ratio) as usize + 2;
            let frame_size = 512;

            let is_paused = (state.engine_flags & (1 << 0)) != 0;
            let enable_steam_audio = (state.engine_flags & (1 << 1)) != 0;
            let enable_occlusion = (state.engine_flags & (1 << 2)) != 0;
            let enable_transmission = (state.engine_flags & (1 << 3)) != 0;

            while state.accum_l.len() < needed_samples {
                let mut mix_l = vec![0.0f32; frame_size];
                let mut mix_r = vec![0.0f32; frame_size];

                let listener_pos = state.listener_pos;
                let listener_fwd = state.listener_fwd;
                let listener_up = state.listener_up;
                let category_volumes = state.category_volumes;
                let engine_flags = state.engine_flags;

                let mut i = 0;
                while i < state.active_sounds.len() {
                    let sound = &mut state.active_sounds[i];
                    let mut finished = false;

                    if is_paused && !sound.is_relative {
                        i += 1;
                        continue;
                    }

                    if sound.is_relative || !sound.is_spatial || !enable_steam_audio {
                        // --- 2D DIRECT MIX (Stereo / Mono / Fallback Spatial) ---
                        let (pan, dist_attenuation) = if !sound.is_relative && sound.is_spatial {
                            let dist_vec = [
                                sound.pos[0] - listener_pos[0],
                                sound.pos[1] - listener_pos[1],
                                sound.pos[2] - listener_pos[2],
                            ];
                            let distance = (dist_vec[0]*dist_vec[0] + dist_vec[1]*dist_vec[1] + dist_vec[2]*dist_vec[2]).sqrt();
                            let p = if distance > 0.1 {
                                let dx = dist_vec[0] / distance;
                                (0.5 + dx * 0.4).clamp(0.1, 0.9)
                            } else {
                                0.5
                            };

                            let max_range = 16.0f32 * sound.volume.max(1.0);
                            let att = if distance < 1.0 {
                                1.0
                            } else if distance >= max_range {
                                0.0
                            } else {
                                (1.0 - (distance - 1.0) / (max_range - 1.0)).max(0.0)
                            };
                            (p, att)
                        } else {
                            (0.5f32, 1.0f32)
                        };

                        for f in 0..frame_size {
                            let cursor_idx = (sound.cursor + f as f32 * sound.pitch_step) as usize;

                            if cursor_idx * (sound.channels as usize) >= sound.pcm.len() {
                                finished = true;
                                break;
                            }

                            let (l_sample, r_sample) = if sound.channels == 1 {
                                let sample = sound.pcm[cursor_idx];
                                (sample, sample)
                            } else {
                                (sound.pcm[cursor_idx * 2], sound.pcm[cursor_idx * 2 + 1])
                            };

                            let live_vol = sound.volume
                                * category_volumes[sound.category_id]
                                * category_volumes[0]
                                * dist_attenuation;

                            mix_l[f] += l_sample * live_vol * (1.0 - pan).sqrt();
                            mix_r[f] += r_sample * live_vol * pan.sqrt();
                        }
                    } else {
                        // --- 3D STEAM AUDIO MIX (Mono Spatialized) ---
                        let mut mono_input = vec![0.0f32; frame_size];
                        for f in 0..frame_size {
                            let cursor_idx = (sound.cursor + f as f32 * sound.pitch_step) as usize;

                            if cursor_idx * (sound.channels as usize) >= sound.pcm.len() {
                                finished = true;
                                break;
                            }

                            if sound.channels == 1 {
                                mono_input[f] = sound.pcm[cursor_idx];
                            } else {
                                let l = sound.pcm[cursor_idx * 2];
                                let r = sound.pcm[cursor_idx * 2 + 1];
                                mono_input[f] = (l + r) * 0.5;
                            }
                        }

                        let dist_vec = [
                            sound.pos[0] - listener_pos[0],
                            sound.pos[1] - listener_pos[1],
                            sound.pos[2] - listener_pos[2],
                        ];
                        let distance = (dist_vec[0]*dist_vec[0] + dist_vec[1]*dist_vec[1] + dist_vec[2]*dist_vec[2]).sqrt();

                        let max_range = 16.0f32 * sound.volume.max(1.0);
                        let distance_attenuation = if distance < 1.0 {
                            1.0
                        } else if distance >= max_range {
                            0.0
                        } else {
                            (1.0 - (distance - 1.0) / (max_range - 1.0)).max(0.0)
                        };

                        let air_absorption = [
                            1.0,
                            ( -0.05 * distance ).exp().max(0.1),
                            ( -0.10 * distance ).exp().max(0.01),
                        ];

                        let occlusion_val = if enable_occlusion { 0.5f32 } else { 0.0f32 };
                        let transmission_val = if enable_transmission { [0.7f32, 0.5f32, 0.3f32] } else { [1.0f32, 1.0f32, 1.0f32] };

                        let mut direct_output = vec![0.0f32; frame_size];
                        sound.direct_effect.apply(
                            &mono_input,
                            distance_attenuation,
                            air_absorption,
                            engine_flags,
                            occlusion_val,
                            transmission_val,
                            &mut direct_output
                        );

                        let direction = steam::get_relative_direction(
                            sound.pos,
                            listener_pos,
                            listener_fwd,
                            listener_up
                        );

                        let mut spatialized_l = vec![0.0f32; frame_size];
                        let mut spatialized_r = vec![0.0f32; frame_size];

                        sound.binaural_effect.apply(
                            &direct_output,
                            direction,
                            app_state.hrtf,
                            &mut spatialized_l,
                            &mut spatialized_r
                        );

                        let live_vol = sound.volume
                            * category_volumes[sound.category_id]
                            * category_volumes[0];

                        for f in 0..frame_size {
                            mix_l[f] += spatialized_l[f] * live_vol;
                            mix_r[f] += spatialized_r[f] * live_vol;
                        }
                    }

                    if finished {
                        state.active_sounds.remove(i);
                        continue;
                    }

                    sound.cursor += frame_size as f32 * sound.pitch_step;
                    i += 1;
                }

                state.accum_l.extend_from_slice(&mix_l);
                state.accum_r.extend_from_slice(&mix_r);
            }

            for f in 0..output_frames {
                let idx = state.resample_cursor as usize;
                let t = state.resample_cursor - idx as f32;

                let sample_l = state.accum_l[idx] * (1.0 - t) + state.accum_l[idx + 1] * t;
                let sample_r = state.accum_r[idx] * (1.0 - t) + state.accum_r[idx + 1] * t;

                data[f * 2] = sample_l;
                data[f * 2 + 1] = sample_r;

                state.resample_cursor += ratio;
            }

            let consumed = state.resample_cursor as usize;
            state.accum_l.drain(0..consumed);
            state.accum_r.drain(0..consumed);
            state.resample_cursor -= consumed as f32;
        },
        |err| eprintln!("[Rust Daemon] CPAL Error: {}", err),
        None
    )
}