use std::sync::Arc;
use std::collections::HashMap;
use cpal::traits::{DeviceTrait, StreamTrait};
use crossbeam_channel::Receiver;

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
    },
    StopSound {
        uid: u32,
    },
    LoadAsset {
        hash: u32,
        asset: PCMAsset,
    },
    UpdateListener {
        pos: [f32; 3],
        fwd: [f32; 3],
        up: [f32; 3],
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
    direct_effect: steam::SteamDirectEffect,
    binaural_effect: steam::SteamBinauralEffect,
}

pub fn run_audio_thread(
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
    app_state: Arc<AppState>,
    rx_cmd: Receiver<AudioCommand>,
) {
    let device_sample_rate = config.sample_rate().0 as f32;
    let output_channels = config.channels() as usize;

    let stream = device.build_output_stream(
        &config.into(),
        {
            let mut active_sounds: Vec<ActiveSound> = Vec::new();
            let mut asset_cache: HashMap<u32, PCMAsset> = HashMap::new();

            let mut listener_pos = [0.0f32; 3];
            let mut listener_fwd = [0.0f32; 3];
            let mut listener_up = [0.0f32; 3];

            let mut accum_l: Vec<f32> = Vec::new();
            let mut accum_r: Vec<f32> = Vec::new();
            let mut resample_cursor = 0.0f32;

            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while let Ok(cmd) = rx_cmd.try_recv() {
                    match cmd {
                        AudioCommand::LoadAsset { hash, asset } => {
                            asset_cache.insert(hash, asset);
                        }
                        AudioCommand::UpdateListener { pos, fwd, up } => {
                            listener_pos = pos;
                            listener_fwd = fwd;
                            listener_up = up;
                        }
                        AudioCommand::StopSound { uid } => {
                            active_sounds.retain(|s| s.uid != uid);
                        }
                        AudioCommand::PlaySound { uid, pos, volume, pitch, asset_hash, is_relative, is_spatial } => {
                            if let Some(cached) = asset_cache.get(&asset_hash) {
                                let base_step = cached.sample_rate as f32 / 48000.0f32;

                                let direct = steam::SteamDirectEffect::new(app_state.context, 48000, 512);
                                let binaural = steam::SteamBinauralEffect::new(
                                    app_state.context,
                                    48000,
                                    512,
                                    app_state.hrtf
                                );

                                active_sounds.push(ActiveSound {
                                    uid,
                                    pcm: Arc::clone(&cached.pcm),
                                    cursor: 0.0,
                                    volume,
                                    pitch_step: base_step * pitch,
                                    channels: cached.channels,
                                    pos,
                                    is_relative,
                                    is_spatial,
                                    direct_effect: direct,
                                    binaural_effect: binaural,
                                });
                            }
                        }
                    }
                }

                data.fill(0.0);

                if output_channels != 2 {
                    return;
                }

                let output_frames = data.len() / 2;
                let ratio = 48000.0f32 / device_sample_rate;

                let needed_samples = (output_frames as f32 * ratio) as usize + 2;
                let frame_size = 512;

                while accum_l.len() < needed_samples {
                    let mut mix_l = vec![0.0f32; frame_size];
                    let mut mix_r = vec![0.0f32; frame_size];

                    let mut i = 0;
                    while i < active_sounds.len() {
                        let sound = &mut active_sounds[i];
                        let mut finished = false;

                        if sound.is_relative || !sound.is_spatial {
                            // --- 2D DIRECT MIX (Stereo / Mono) ---
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

                                mix_l[f] += l_sample * sound.volume;
                                mix_r[f] += r_sample * sound.volume;
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

                            // Notice we no longer break/continue early here. We process the effects
                            // for whatever data has successfully gathered inside `mono_input`.

                            let dist_vec = [
                                sound.pos[0] - listener_pos[0],
                                sound.pos[1] - listener_pos[1],
                                sound.pos[2] - listener_pos[2],
                            ];
                            let distance = (dist_vec[0]*dist_vec[0] + dist_vec[1]*dist_vec[1] + dist_vec[2]*dist_vec[2]).sqrt();

                            let min_distance = 1.0f32;
                            let distance_attenuation = if distance < min_distance {
                                1.0
                            } else {
                                min_distance / distance
                            };

                            let air_absorption = [
                                1.0,
                                ( -0.05 * distance ).exp().max(0.1),
                                ( -0.10 * distance ).exp().max(0.01),
                            ];

                            let mut direct_output = vec![0.0f32; frame_size];
                            sound.direct_effect.apply(&mono_input, distance_attenuation, air_absorption, &mut direct_output);

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

                            for f in 0..frame_size {
                                mix_l[f] += spatialized_l[f] * sound.volume;
                                mix_r[f] += spatialized_r[f] * sound.volume;
                            }
                        }

                        // Remove the sound after completing both 2D/3D operations
                        if finished {
                            active_sounds.remove(i);
                            continue;
                        }

                        sound.cursor += frame_size as f32 * sound.pitch_step;
                        i += 1;
                    }

                    accum_l.extend_from_slice(&mix_l);
                    accum_r.extend_from_slice(&mix_r);
                }

                for f in 0..output_frames {
                    let idx = resample_cursor as usize;
                    let t = resample_cursor - idx as f32;

                    let sample_l = accum_l[idx] * (1.0 - t) + accum_l[idx + 1] * t;
                    let sample_r = accum_r[idx] * (1.0 - t) + accum_r[idx + 1] * t;

                    data[f * 2] = sample_l;
                    data[f * 2 + 1] = sample_r;

                    resample_cursor += ratio;
                }

                let consumed = resample_cursor as usize;
                accum_l.drain(0..consumed);
                accum_r.drain(0..consumed);
                resample_cursor -= consumed as f32;
            }
        },
        |err| eprintln!("[Rust Daemon] CPAL Error: {}", err),
        None
    ).expect("Failed to build output stream!");

    stream.play().expect("Failed to start stream!");

    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}