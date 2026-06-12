use std::sync::Arc;
use std::collections::HashMap;
use cpal::traits::DeviceTrait;

use crate::AppState;
use super::source::{ActiveSound, AudioSource};
use super::AudioCommand;

const FRAME_SIZE: usize = 512;
const MAX_STREAM_BUFFER: usize = 48000 * 3;

#[cfg(windows)]
extern "system" { fn CoInitializeEx(pvReserved: *mut std::ffi::c_void, dwCoInit: u32) -> i32; }

struct PendingSound {
    uid: u32, pos: [f32; 3], volume: f32, pitch: f32, asset_hash: u32,
    is_relative: bool, is_spatial: bool, category_id: usize,
    direct_effect: Option<crate::phonon::SteamDirectEffect>,
    binaural_effect: Option<crate::phonon::SteamBinauralEffect>,
    ipl_source: Option<crate::phonon::SteamSource>
}

struct AudioState {
    active_sounds: Vec<ActiveSound>,
    pending_sounds: Vec<PendingSound>,
    asset_cache: HashMap<u32, crate::asset::PCMAsset>,
    stream_senders: HashMap<u32, crossbeam_channel::Sender<Vec<f32>>>,
    listener_pos: [f32; 3], category_volumes: [f32; 16], engine_flags: u32,
    accum_l: Vec<f32>, accum_r: Vec<f32>, resample_cursor: f32,
}

pub fn run_audio_thread(device: cpal::Device, config: cpal::SupportedStreamConfig, app_state: Arc<AppState>, rx_cmd: crossbeam_channel::Receiver<AudioCommand>) {
    #[cfg(windows)] unsafe { let _ = CoInitializeEx(std::ptr::null_mut(), 0x0); }

    let device_sample_rate = config.sample_rate().0 as f32;
    let output_channels = config.channels() as usize;

    let mut state = AudioState {
        active_sounds: Vec::new(), pending_sounds: Vec::new(), asset_cache: HashMap::new(), stream_senders: HashMap::new(),
        listener_pos: [0.0; 3], category_volumes: [1.0; 16], engine_flags: 0,
        accum_l: Vec::new(), accum_r: Vec::new(), resample_cursor: 0.0,
    };

    let stream_result = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _| {
            data.fill(0.0);
            if output_channels != 2 { return; }

            while let Ok(cmd) = rx_cmd.try_recv() {
                match cmd {
                    AudioCommand::UpdateListener { pos, fwd, up, category_volumes, engine_flags } => {
                        state.listener_pos = pos; state.category_volumes = category_volumes; state.engine_flags = engine_flags;
                        let mut inputs: crate::steam_audio::IPLSimulationSharedInputs = unsafe { std::mem::zeroed() };
                        inputs.listener.origin = crate::steam_audio::IPLVector3 { x: pos[0], y: pos[1], z: pos[2] };
                        inputs.listener.ahead = crate::steam_audio::IPLVector3 { x: fwd[0], y: fwd[1], z: fwd[2] };
                        inputs.listener.up = crate::steam_audio::IPLVector3 { x: up[0], y: up[1], z: up[2] };
                        inputs.listener.right = crate::steam_audio::IPLVector3 {
                            x: up[1]*fwd[2] - up[2]*fwd[1], y: up[2]*fwd[0] - up[0]*fwd[2], z: up[0]*fwd[1] - up[1]*fwd[0],
                        };
                        unsafe {
                            let _guard = app_state.simulator_mutex.lock().unwrap_or_else(|e| e.into_inner());
                            crate::steam_audio::iplSimulatorSetSharedInputs(
                                app_state.simulator,
                                crate::steam_audio::IPLSimulationFlags_IPL_SIMULATIONFLAGS_DIRECT | crate::steam_audio::IPLSimulationFlags_IPL_SIMULATIONFLAGS_REFLECTIONS,
                                &mut inputs
                            );
                            crate::steam_audio::iplSimulatorCommit(app_state.simulator);
                        }
                    }
                    AudioCommand::PlaySound { uid, pos, volume, pitch, asset_hash, is_relative, is_spatial, category_id, direct_effect, binaural_effect, ipl_source } => {
                        if let Some(cached) = state.asset_cache.get(&asset_hash) {
                            let pcm = Arc::clone(&cached.pcm);
                            let pitch_step = pitch * (cached.sample_rate as f32 / 48000.0f32);
                            state.active_sounds.push(ActiveSound { uid, source: AudioSource::Static { pcm, cursor: 0.0 }, volume, pitch_step, channels: cached.channels, pos, is_relative, is_spatial, category_id, direct_effect, binaural_effect, ipl_source });
                        } else {
                            state.pending_sounds.push(PendingSound { uid, pos, volume, pitch, asset_hash, is_relative, is_spatial, category_id, direct_effect, binaural_effect, ipl_source });
                            if state.pending_sounds.len() > 100 { state.pending_sounds.remove(0); }
                        }
                    }
                    AudioCommand::LoadAsset { hash, asset } => {
                        state.asset_cache.insert(hash, asset);
                        let mut i = 0;
                        while i < state.pending_sounds.len() {
                            if state.pending_sounds[i].asset_hash == hash {
                                let p = state.pending_sounds.remove(i);
                                let cached = state.asset_cache.get(&hash).unwrap();
                                let pitch_step = p.pitch * (cached.sample_rate as f32 / 48000.0f32);
                                state.active_sounds.push(ActiveSound { uid: p.uid, source: AudioSource::Static { pcm: Arc::clone(&cached.pcm), cursor: 0.0 }, volume: p.volume, pitch_step, channels: cached.channels, pos: p.pos, is_relative: p.is_relative, is_spatial: p.is_spatial, category_id: p.category_id, direct_effect: p.direct_effect, binaural_effect: p.binaural_effect, ipl_source: p.ipl_source });
                            } else { i += 1; }
                        }
                    }
                    AudioCommand::PlayStream { uid, pos, volume, pitch, is_relative, is_spatial, category_id, sample_rate, channels, direct_effect, binaural_effect, ipl_source } => {
                        let (tx, rx) = crossbeam_channel::unbounded::<Vec<f32>>();
                        state.stream_senders.insert(uid, tx);
                        state.active_sounds.push(ActiveSound { uid, source: AudioSource::Stream { rx, buffer: Vec::new(), cursor: 0.0 }, volume, pitch_step: pitch * (sample_rate as f32 / 48000.0f32), channels, pos, is_relative, is_spatial, category_id, direct_effect, binaural_effect, ipl_source });
                    }
                    AudioCommand::QueueStreamData { uid, samples } => { if let Some(tx) = state.stream_senders.get(&uid) { let _ = tx.send(samples); } }
                    AudioCommand::StopSound { uid } => { state.active_sounds.retain(|s| s.uid != uid); state.pending_sounds.retain(|s| s.uid != uid); state.stream_senders.remove(&uid); }
                    AudioCommand::StopAllSounds => { state.active_sounds.clear(); state.pending_sounds.clear(); state.stream_senders.clear(); }
                    AudioCommand::ChangeDevice { .. } => {}
                }
            }

            let needed_samples = (data.len() as f32 / 2.0 * (48000.0 / device_sample_rate)) as usize + 2;

            while state.accum_l.len() < needed_samples {
                let mut mix_l = [0.0f32; FRAME_SIZE];
                let mut mix_r = [0.0f32; FRAME_SIZE];

                let mut i = 0;
                while i < state.active_sounds.len() {
                    let sound = &mut state.active_sounds[i];
                    let mut finished = false;
                    let mut sound_frame_mono = [0.0f32; FRAME_SIZE];
                    let mut sound_frame_l = [0.0f32; FRAME_SIZE];
                    let mut sound_frame_r = [0.0f32; FRAME_SIZE];

                    match &mut sound.source {
                        AudioSource::Static { pcm, cursor } => {
                            for f in 0..FRAME_SIZE {
                                let cursor_idx = (*cursor + f as f32 * sound.pitch_step) as usize;
                                if cursor_idx * (sound.channels as usize) >= pcm.len() { finished = true; break; }
                                if sound.channels == 1 {
                                    let sample = pcm[cursor_idx];
                                    sound_frame_mono[f] = sample; sound_frame_l[f] = sample; sound_frame_r[f] = sample;
                                } else {
                                    let l = pcm[cursor_idx * 2]; let r = pcm[cursor_idx * 2 + 1];
                                    sound_frame_l[f] = l; sound_frame_r[f] = r; sound_frame_mono[f] = (l + r) * 0.5;
                                }
                            }
                            if !finished { *cursor += FRAME_SIZE as f32 * sound.pitch_step; }
                        }
                        AudioSource::Stream { rx, buffer, cursor } => {
                            while let Ok(mut chunk) = rx.try_recv() { buffer.append(&mut chunk); }

                            // CRITICAL FIX: Safe float math limits
                            if buffer.len() > MAX_STREAM_BUFFER {
                                let excess = buffer.len() - MAX_STREAM_BUFFER;
                                buffer.drain(0..excess);
                                *cursor = (*cursor - excess as f32).max(0.0);
                            }

                            for f in 0..FRAME_SIZE {
                                let next_pos = *cursor + f as f32 * sound.pitch_step;
                                let idx = next_pos as usize;
                                let t = next_pos - idx as f32;
                                let val = if idx + 1 < buffer.len() { buffer[idx] * (1.0 - t) + buffer[idx + 1] * t } else if idx < buffer.len() { buffer[idx] } else { 0.0 };
                                sound_frame_mono[f] = val; sound_frame_l[f] = val; sound_frame_r[f] = val;
                            }
                            *cursor += FRAME_SIZE as f32 * sound.pitch_step;
                            if *cursor > buffer.len() as f32 { *cursor = buffer.len() as f32; }
                            let consumed_idx = *cursor as usize;
                            if consumed_idx > 0 { let safe_drain = consumed_idx.min(buffer.len()); buffer.drain(0..safe_drain); *cursor -= safe_drain as f32; }
                        }
                    }

                    if finished { state.active_sounds.remove(i); continue; }

                    if sound.is_relative || !sound.is_spatial || (state.engine_flags & (1 << 1)) == 0 {
                        let mut pan = 0.5; let mut att = 1.0;
                        if !sound.is_relative && sound.is_spatial {
                            let d_vec = [sound.pos[0] - state.listener_pos[0], sound.pos[1] - state.listener_pos[1], sound.pos[2] - state.listener_pos[2]];
                            let dist = (d_vec[0]*d_vec[0] + d_vec[1]*d_vec[1] + d_vec[2]*d_vec[2]).sqrt();
                            if dist > 0.1 { pan = (0.5 + (d_vec[0]/dist)*0.4).clamp(0.1, 0.9); }
                            let max_range = 16.0 * sound.volume.max(1.0);
                            att = if dist < 1.0 { 1.0 } else if dist >= max_range { 0.0 } else { (1.0 - (dist - 1.0)/(max_range - 1.0)).max(0.0) };
                        }
                        for f in 0..FRAME_SIZE {
                            let live_vol = sound.volume * state.category_volumes[sound.category_id] * state.category_volumes[0] * att;
                            mix_l[f] += sound_frame_l[f] * live_vol * (1.0 - pan).sqrt();
                            mix_r[f] += sound_frame_r[f] * live_vol * pan.sqrt();
                        }
                    } else {
                        let mut sim_outputs = unsafe { std::mem::zeroed() };
                        unsafe {
                            if let Some(ipl_src) = &sound.ipl_source {
                                if !ipl_src.source.is_null() {
                                    let _guard = app_state.simulator_mutex.lock().unwrap_or_else(|e| e.into_inner());
                                    crate::steam_audio::iplSourceGetOutputs(ipl_src.source, crate::steam_audio::IPLSimulationFlags_IPL_SIMULATIONFLAGS_DIRECT, &mut sim_outputs);
                                }
                            }
                        }

                        let mut direct_output = [0.0f32; FRAME_SIZE];
                        if let Some(direct) = &mut sound.direct_effect {
                            direct.apply(&sound_frame_mono, &sim_outputs, state.engine_flags, &mut direct_output);
                        } else {
                            direct_output.copy_from_slice(&sound_frame_mono);
                        }

                        let mut spatialized_l = [0.0f32; FRAME_SIZE];
                        let mut spatialized_r = [0.0f32; FRAME_SIZE];

                        // CRITICAL FIX: Normalize direction vector to prevent HRTF division-by-zero segfaults
                        let mut d_vec = [sound.pos[0] - state.listener_pos[0], sound.pos[1] - state.listener_pos[1], sound.pos[2] - state.listener_pos[2]];
                        let dist = (d_vec[0]*d_vec[0] + d_vec[1]*d_vec[1] + d_vec[2]*d_vec[2]).sqrt();
                        let direction = if dist > 0.001 {
                            crate::steam_audio::IPLVector3 { x: d_vec[0] / dist, y: d_vec[1] / dist, z: d_vec[2] / dist }
                        } else {
                            crate::steam_audio::IPLVector3 { x: 0.0, y: 0.0, z: 1.0 } // Safe fallback forward vector
                        };

                        if let Some(binaural) = &mut sound.binaural_effect {
                            binaural.apply(&direct_output, direction, app_state.hrtf, &mut spatialized_l, &mut spatialized_r);
                        } else {
                            spatialized_l.copy_from_slice(&direct_output);
                            spatialized_r.copy_from_slice(&direct_output);
                        }

                        let live_vol = sound.volume * state.category_volumes[sound.category_id] * state.category_volumes[0];
                        for f in 0..FRAME_SIZE { mix_l[f] += spatialized_l[f] * live_vol; mix_r[f] += spatialized_r[f] * live_vol; }
                    }
                    i += 1;
                }
                state.accum_l.extend_from_slice(&mix_l); state.accum_r.extend_from_slice(&mix_r);
            }

            for f in 0..(data.len() / 2) {
                let idx = state.resample_cursor as usize;
                let t = state.resample_cursor - idx as f32;
                let sl = if idx + 1 < state.accum_l.len() { state.accum_l[idx] * (1.0 - t) + state.accum_l[idx + 1] * t } else if idx < state.accum_l.len() { state.accum_l[idx] } else { 0.0 };
                let sr = if idx + 1 < state.accum_r.len() { state.accum_r[idx] * (1.0 - t) + state.accum_r[idx + 1] * t } else if idx < state.accum_r.len() { state.accum_r[idx] } else { 0.0 };
                data[f * 2] = sl; data[f * 2 + 1] = sr;
                state.resample_cursor += 48000.0 / device_sample_rate;
            }
            let consumed = state.resample_cursor as usize;
            let final_consumed = consumed.min(state.accum_l.len()).min(state.accum_r.len());
            state.accum_l.drain(0..final_consumed); state.accum_r.drain(0..final_consumed);
            state.resample_cursor -= final_consumed as f32;
        },
        |err| eprintln!("[Rust Daemon] CPAL Error: {}", err),
        None
    );

    match stream_result {
        Ok(stream) => {
            if let Err(err) = cpal::traits::StreamTrait::play(&stream) { eprintln!("[Rust Daemon] Critical failure: Could not start CPAL: {:?}", err); return; }
            std::thread::park();
        }
        Err(err) => eprintln!("[Rust Daemon] Critical failure: Could not build CPAL: {:?}", err),
    }
}