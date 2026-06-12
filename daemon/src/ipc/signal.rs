use tokio::io::AsyncReadExt;
use byteorder::{LittleEndian, ByteOrder};
use crossbeam_channel::Sender;
use std::sync::Arc;
use crate::AppState;
use crate::audio::AudioCommand;
use crate::asset;

// FIX: Raised limit to 128MB to safely support large background music, ambient loops, and custom music discs
const MAX_PAYLOAD_SIZE: usize = 134217728;

pub async fn handle_client_stream<S>(mut stream: S, app_state: Arc<AppState>, tx_cmd: Sender<AudioCommand>)
where S: AsyncReadExt + Unpin {
    let mut buffer = vec![0u8; 65536];
    let mut pending_data = Vec::new();

    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => {
                eprintln!("[Rust Daemon] Native Signal Channel closed by JVM.");
                return;
            }
            Ok(bytes_read) => {
                pending_data.extend_from_slice(&buffer[..bytes_read]);

                while pending_data.len() >= 13 {
                    if &pending_data[0..4] != b"DCBL" {
                        pending_data.remove(0);
                        continue;
                    }

                    let opcode = pending_data[4];
                    let hash = LittleEndian::read_u32(&pending_data[5..9]);
                    let payload_size = LittleEndian::read_u32(&pending_data[9..13]) as usize;

                    if payload_size > MAX_PAYLOAD_SIZE {
                        eprintln!("[Rust Daemon] Security check: Payload size ({:.2} MB) exceeds limits. Stream is corrupted or too large. Closing socket.", payload_size as f32 / 1024.0 / 1024.0);
                        return;
                    }

                    if pending_data.len() < 13 + payload_size { break; }

                    let payload = pending_data[13 .. 13 + payload_size].to_vec();
                    pending_data.drain(0 .. 13 + payload_size);

                    if opcode == 1 {
                        let tx_cmd_task = tx_cmd.clone();
                        tokio::task::spawn_blocking(move || {
                            if let Ok(pcm_asset) = asset::decode_ogg_in_memory(payload) {
                                let _ = tx_cmd_task.send(AudioCommand::LoadAsset { hash, asset: pcm_asset });
                            } else {
                                eprintln!("[Rust Daemon] Failed to decode OGG payload for hash {}.", hash);
                            }
                        });
                    } else if opcode == 2 {
                        if payload.len() >= 28 {
                            let x = LittleEndian::read_f32(&payload[0..4]);
                            let y = LittleEndian::read_f32(&payload[4..8]);
                            let z = LittleEndian::read_f32(&payload[8..12]);
                            let volume = LittleEndian::read_f32(&payload[12..16]);
                            let pitch = LittleEndian::read_f32(&payload[16..20]);
                            let is_relative = payload[20] != 0;
                            let is_spatial = payload[21] != 0;
                            let category_id = payload[22] as usize;
                            let sample_rate = LittleEndian::read_u32(&payload[23..27]);
                            let channels = payload[27] as u16;

                            let (direct_effect, binaural_effect, ipl_source) = if !is_relative && is_spatial {
                                let direct = crate::phonon::SteamDirectEffect::new(app_state.context, 48000, 512);
                                let binaural = crate::phonon::SteamBinauralEffect::new(app_state.context, 48000, 512, app_state.hrtf);
                                let ipl_src = {
                                    let _guard = app_state.simulator_mutex.lock().unwrap_or_else(|e| e.into_inner());
                                    let src = crate::phonon::create_ipl_source(app_state.simulator, [x,y,z], Arc::clone(&app_state.simulator_mutex));
                                    unsafe { crate::steam_audio::iplSimulatorCommit(app_state.simulator); }
                                    src
                                };
                                (direct, binaural, ipl_src)
                            } else {
                                (None, None, None)
                            };

                            let _ = tx_cmd.send(AudioCommand::PlayStream {
                                uid: hash, pos: [x, y, z], volume, pitch, is_relative, is_spatial, category_id,
                                sample_rate, channels, direct_effect, binaural_effect, ipl_source
                            });
                        }
                    } else if opcode == 3 {
                        let num_samples = payload.len() / 4;
                        let mut samples = Vec::with_capacity(num_samples);
                        for chunk in payload.chunks_exact(4) { samples.push(LittleEndian::read_f32(chunk)); }
                        let _ = tx_cmd.send(AudioCommand::QueueStreamData { uid: hash, samples });
                    }
                }
            }
            Err(e) => {
                eprintln!("[Rust Daemon] Socket read error: {}. Disconnecting client.", e);
                return;
            }
        }
    }
}