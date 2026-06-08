use std::sync::{Arc, Mutex};
use cpal::traits::DeviceTrait;

use crate::AppState;
use crate::phonon;
use super::SharedAudioState;
use super::reverb::FdnReverb;
use super::source::AudioSource;

const FRAME_SIZE: usize = 512;

pub fn build_stream(
    device: &cpal::Device, config: &cpal::SupportedStreamConfig,
    state: Arc<Mutex<SharedAudioState>>, app_state: Arc<AppState>,
) -> cpal::Stream {
    build_stream_result(device, config, state, app_state).expect("Failed to initialize CPAL Stream")
}

pub fn build_stream_result(
    device: &cpal::Device, config: &cpal::SupportedStreamConfig,
    state: Arc<Mutex<SharedAudioState>>, app_state: Arc<AppState>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {

    let device_sample_rate = config.sample_rate().0 as f32;
    let output_channels = config.channels() as usize;

    let mut reverb_l = FdnReverb::new();
    let mut reverb_r = FdnReverb::new();

    device.build_output_stream(
        &config.clone().into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            data.fill(0.0);
            if output_channels != 2 { return; }

            let mut state = match state.try_lock() {
                Ok(s) => s,
                Err(_) => return,
            };

            let output_frames = data.len() / 2;
            let ratio = 48000.0f32 / device_sample_rate;
            let needed_samples = (output_frames as f32 * ratio) as usize + 2;

            let is_paused = (state.engine_flags & (1 << 0)) != 0;
            let enable_steam_audio = (state.engine_flags & (1 << 1)) != 0;
            let enable_occlusion = (state.engine_flags & (1 << 2)) != 0;
            let enable_transmission = (state.engine_flags & (1 << 3)) != 0;
            let enable_reverb = (state.engine_flags & (1 << 4)) != 0;

            let mut mix_l = [0.0f32; FRAME_SIZE];
            let mut mix_r = [0.0f32; FRAME_SIZE];
            let mut mono_input = [0.0f32; FRAME_SIZE];
            let mut direct_output = [0.0f32; FRAME_SIZE];
            let mut spatialized_l = [0.0f32; FRAME_SIZE];
            let mut spatialized_r = [0.0f32; FRAME_SIZE];

            let mut sound_frame_mono = [0.0f32; FRAME_SIZE];
            let mut sound_frame_l = [0.0f32; FRAME_SIZE];
            let mut sound_frame_r = [0.0f32; FRAME_SIZE];

            while state.accum_l.len() < needed_samples {
                mix_l.fill(0.0);
                mix_r.fill(0.0);

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

                    sound_frame_l.fill(0.0);
                    sound_frame_r.fill(0.0);
                    sound_frame_mono.fill(0.0);

                    match &mut sound.source {
                        AudioSource::Static { pcm, cursor } => {
                            for f in 0..FRAME_SIZE {
                                let cursor_idx = (*cursor + f as f32 * sound.pitch_step) as usize;
                                if cursor_idx * (sound.channels as usize) >= pcm.len() {
                                    finished = true;
                                    break;
                                }

                                if sound.channels == 1 {
                                    let sample = pcm[cursor_idx];
                                    sound_frame_mono[f] = sample;
                                    sound_frame_l[f] = sample;
                                    sound_frame_r[f] = sample;
                                } else {
                                    let l = pcm[cursor_idx * 2];
                                    let r = pcm[cursor_idx * 2 + 1];
                                    sound_frame_l[f] = l;
                                    sound_frame_r[f] = r;
                                    sound_frame_mono[f] = (l + r) * 0.5;
                                }
                            }
                            if !finished {
                                *cursor += FRAME_SIZE as f32 * sound.pitch_step;
                            }
                        }
                        AudioSource::Stream { rx, buffer, cursor } => {
                            while let Ok(mut chunk) = rx.try_recv() {
                                buffer.append(&mut chunk);
                            }

                            for f in 0..FRAME_SIZE {
                                let next_pos = *cursor + f as f32 * sound.pitch_step;
                                let idx = next_pos as usize;
                                let t = next_pos - idx as f32;

                                let val = if idx + 1 < buffer.len() {
                                    buffer[idx] * (1.0 - t) + buffer[idx + 1] * t
                                } else if idx < buffer.len() {
                                    buffer[idx]
                                } else {
                                    0.0
                                };

                                sound_frame_mono[f] = val;
                                sound_frame_l[f] = val;
                                sound_frame_r[f] = val;
                            }

                            let consumed_float = FRAME_SIZE as f32 * sound.pitch_step;
                            *cursor += consumed_float;
                            let consumed_idx = *cursor as usize;

                            // BUG FIX: Bounds checking that prevents desync crashes/leaks
                            if consumed_idx > 0 {
                                let safe_drain = consumed_idx.min(buffer.len());
                                buffer.drain(0..safe_drain);
                                *cursor -= safe_drain as f32;
                            }
                        }
                    }

                    if finished {
                        state.active_sounds.remove(i);
                        continue;
                    }

                    if sound.is_relative || !sound.is_spatial || !enable_steam_audio {
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
                            let att = if distance < 1.0 { 1.0 } else if distance >= max_range { 0.0 } else {
                                (1.0 - (distance - 1.0) / (max_range - 1.0)).max(0.0)
                            };
                            (p, att)
                        } else {
                            (0.5f32, 1.0f32)
                        };

                        for f in 0..FRAME_SIZE {
                            let live_vol = sound.volume * category_volumes[sound.category_id] * category_volumes[0] * dist_attenuation;
                            mix_l[f] += sound_frame_l[f] * live_vol * (1.0 - pan).sqrt();
                            mix_r[f] += sound_frame_r[f] * live_vol * pan.sqrt();
                        }
                    } else {
                        mono_input.copy_from_slice(&sound_frame_mono);

                        let dist_vec = [
                            sound.pos[0] - listener_pos[0], sound.pos[1] - listener_pos[1], sound.pos[2] - listener_pos[2],
                        ];
                        let distance = (dist_vec[0]*dist_vec[0] + dist_vec[1]*dist_vec[1] + dist_vec[2]*dist_vec[2]).sqrt();
                        let max_range = 16.0f32 * sound.volume.max(1.0);
                        let distance_attenuation = if distance < 1.0 { 1.0 } else if distance >= max_range { 0.0 } else {
                            (1.0 - (distance - 1.0) / (max_range - 1.0)).max(0.0)
                        };

                        let air_absorption = [
                            1.0, ( -0.05 * distance ).exp().max(0.1), ( -0.10 * distance ).exp().max(0.01),
                        ];

                        let (occlusion_val, transmission_val) = if enable_occlusion || enable_transmission {
                            phonon::calculate_occlusion_and_transmission(app_state.scene, sound.pos, listener_pos)
                        } else {
                            (1.0f32, [1.0f32, 1.0f32, 1.0f32])
                        };

                        let live_occlusion = if enable_occlusion { occlusion_val } else { 1.0f32 };
                        let live_transmission = if enable_transmission { transmission_val } else { [1.0f32; 3] };

                        direct_output.fill(0.0);
                        sound.direct_effect.apply(
                            &mono_input, distance_attenuation, air_absorption, engine_flags,
                            live_occlusion, live_transmission, &mut direct_output
                        );

                        let direction = phonon::get_relative_direction(
                            app_state.context, sound.pos, listener_pos, listener_fwd, listener_up
                        );

                        spatialized_l.fill(0.0);
                        spatialized_r.fill(0.0);

                        sound.binaural_effect.apply(
                            &direct_output, direction, app_state.hrtf, &mut spatialized_l, &mut spatialized_r
                        );

                        let live_vol = sound.volume * category_volumes[sound.category_id] * category_volumes[0];

                        for f in 0..FRAME_SIZE {
                            mix_l[f] += spatialized_l[f] * live_vol;
                            mix_r[f] += spatialized_r[f] * live_vol;
                        }
                    }
                    i += 1;
                }

                if enable_reverb {
                    let (t60, wet_mix) = phonon::ACTIVE_ENVIRONMENT.load_relaxed();
                    for f in 0..FRAME_SIZE {
                        let wet_l = reverb_l.process(mix_l[f], t60, wet_mix);
                        let wet_r = reverb_r.process(mix_r[f], t60, wet_mix);
                        mix_l[f] += wet_l;
                        mix_r[f] += wet_r;
                    }
                }

                state.accum_l.extend_from_slice(&mix_l);
                state.accum_r.extend_from_slice(&mix_r);
            }

            for f in 0..output_frames {
                let idx = state.resample_cursor as usize;
                let t = state.resample_cursor - idx as f32;

                let sample_l = if idx + 1 < state.accum_l.len() {
                    state.accum_l[idx] * (1.0 - t) + state.accum_l[idx + 1] * t
                } else if idx < state.accum_l.len() {
                    state.accum_l[idx]
                } else { 0.0 };

                let sample_r = if idx + 1 < state.accum_r.len() {
                    state.accum_r[idx] * (1.0 - t) + state.accum_r[idx + 1] * t
                } else if idx < state.accum_r.len() {
                    state.accum_r[idx]
                } else { 0.0 };

                data[f * 2] = sample_l;
                data[f * 2 + 1] = sample_r;
                state.resample_cursor += ratio;
            }

            let consumed = state.resample_cursor as usize;
            let final_consumed = consumed.min(state.accum_l.len()).min(state.accum_r.len());
            state.accum_l.drain(0..final_consumed);
            state.accum_r.drain(0..final_consumed);
            state.resample_cursor -= final_consumed as f32;
        },
        |err| eprintln!("[Rust Daemon] CPAL Error: {}", err),
        None
    )
}