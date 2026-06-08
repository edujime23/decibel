use tokio::io::AsyncReadExt;
use byteorder::{LittleEndian, ByteOrder};
use crossbeam_channel::Sender;
use crate::audio::AudioCommand;
use crate::asset;

pub async fn handle_client_stream<S>(mut stream: S, tx_cmd: Sender<AudioCommand>)
where
    S: AsyncReadExt + Unpin
{
    let mut buffer = vec![0u8; 65536];
    let mut pending_data = Vec::new();

    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => {
                println!("[Rust Daemon] Signal channel disconnected. Initiating fast exit.");
                std::process::exit(0);
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

                    if pending_data.len() < 13 + payload_size {
                        break;
                    }

                    let payload = pending_data[13 .. 13 + payload_size].to_vec();
                    pending_data.drain(0 .. 13 + payload_size);

                    if opcode == 1 {
                        println!("[Rust Daemon] Streamed asset received for hash: {}", hash);
                        let tx_cmd_task = tx_cmd.clone();
                        tokio::task::spawn_blocking(move || {
                            if let Ok(pcm_asset) = asset::decode_ogg_in_memory(payload) {
                                let _ = tx_cmd_task.send(AudioCommand::LoadAsset { hash, asset: pcm_asset });
                            }
                        });
                    } else if opcode == 2 {
                        // BUG FIX: Format explicitly pulls exact sample rates dynamic formatting
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

                            let _ = tx_cmd.send(AudioCommand::PlayStream {
                                uid: hash, pos: [x, y, z], volume, pitch, is_relative, is_spatial, category_id,
                                sample_rate, channels,
                            });
                        }
                    } else if opcode == 3 {
                        let num_samples = payload.len() / 4;
                        let mut samples = Vec::with_capacity(num_samples);
                        for chunk in payload.chunks_exact(4) {
                            samples.push(LittleEndian::read_f32(chunk));
                        }

                        let _ = tx_cmd.send(AudioCommand::QueueStreamData {
                            uid: hash, samples,
                        });
                    }
                }
            }
            Err(e) => {
                eprintln!("[Rust Daemon] IPC read error: {:?}", e);
                std::process::exit(1);
            }
        }
    }
}