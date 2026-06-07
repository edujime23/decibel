use std::sync::Arc;
use std::path::Path;
use std::fs::OpenOptions;
use std::sync::atomic::{compiler_fence, Ordering};
use byteorder::{LittleEndian, ByteOrder};
use memmap2::MmapMut;
use tokio::io::AsyncReadExt;
use crossbeam_channel::Sender;

use crate::AppState;
use crate::audio::AudioCommand;
use crate::asset;

const QUEUE_CAPACITY: usize = 1024;
const SLOT_SIZE: usize = 64;

// Cache-Line Aligned Header Structure (Offsets in Bytes)
const OFFSET_JAVA_WRITE_SEQ: usize      = 0;
const OFFSET_RUST_READ_SEQ: usize       = 64;
const OFFSET_VER: usize                 = 128;
const OFFSET_DEV_SEQ: usize             = 192;
const OFFSET_DEV_NAME: usize            = 196;
const OFFSET_VOXEL_GRID_VERSION: usize  = 320;
const OFFSET_CENTER_X: usize            = 324;
const OFFSET_CENTER_Y: usize            = 328;
const OFFSET_CENTER_Z: usize            = 332;

const HEADER_SIZE: usize                = 512;
const RING_BUFFER_SIZE: usize           = QUEUE_CAPACITY * SLOT_SIZE;
const OFFSET_VOXEL_GRID: usize          = HEADER_SIZE + RING_BUFFER_SIZE; // 66,048

pub async fn run_ipc_loop(shm_path: String, app_state: Arc<AppState>, tx_cmd: Sender<AudioCommand>) {
    let shm_file_path = Path::new(&shm_path);
    let _tmp_dir = shm_file_path.parent().expect("Invalid SHM path parent directory");

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(shm_file_path)
        .expect("Failed to open Shared Memory file");
    let mut mmap = unsafe { MmapMut::map_mut(&file).expect("Failed to map Shared Memory") };

    let tx_cmd_clone = tx_cmd.clone();

    #[cfg(unix)]
    let tmp_dir_clone = _tmp_dir.to_path_buf();

    tokio::spawn(async move {
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            let pipe_name = r"\\.\pipe\decibel_engine";
            let server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(pipe_name)
                .expect("Failed to create Windows Named Pipe");

            println!("[Rust Daemon] Windows Named Pipe listening at {}", pipe_name);
            if server.connect().await.is_ok() {
                handle_client_stream(server, tx_cmd_clone).await;
            }
        }

        #[cfg(unix)]
        {
            use tokio::net::UnixListener;
            let socket_path = tmp_dir_clone.join("decibel_engine.sock");
            if socket_path.exists() {
                let _ = std::fs::remove_file(&socket_path);
            }
            let listener = UnixListener::bind(&socket_path).expect("Failed to bind Unix Domain Socket");
            println!("[Rust Daemon] Unix Domain Socket listening at {}", socket_path.display());
            if let Ok((stream, _)) = listener.accept().await {
                handle_client_stream(stream, tx_cmd_clone).await;
            }
        }
    });

    println!("[Rust Daemon] SHM Control Loop active.");
    let mut local_read_seq = 0;
    let mut local_voxel_version = 0;

    let mut last_pos = [0.0f32; 3];
    let mut last_fwd = [0.0f32; 3];
    let mut last_up = [0.0f32; 3];
    let mut last_category_volumes = [1.0f32; 16];
    let mut last_engine_flags = 0u32;
    let mut last_dev_seq = 0u32;

    loop {
        let java_write_seq = LittleEndian::read_u32(&mmap[OFFSET_JAVA_WRITE_SEQ .. OFFSET_JAVA_WRITE_SEQ + 4]);

        if java_write_seq.saturating_sub(local_read_seq) > QUEUE_CAPACITY as u32 {
            local_read_seq = java_write_seq;
        }

        let mut listener_pos = [0.0f32; 3];
        let mut listener_fwd = [0.0f32; 3];
        let mut listener_up = [0.0f32; 3];
        let mut category_volumes = [1.0f32; 16];

        let mut ver = LittleEndian::read_u32(&mmap[OFFSET_VER .. OFFSET_VER + 4]);
        let mut attempts = 0;

        while ver % 2 != 0 && attempts < 100 {
            std::hint::spin_loop();
            ver = LittleEndian::read_u32(&mmap[OFFSET_VER .. OFFSET_VER + 4]);
            attempts += 1;
        }

        compiler_fence(Ordering::Acquire);

        for idx in 0..3 {
            listener_pos[idx] = LittleEndian::read_f32(&mmap[12 + idx * 4 .. 16 + idx * 4]);
            listener_fwd[idx] = LittleEndian::read_f32(&mmap[24 + idx * 4 .. 28 + idx * 4]);
            listener_up[idx] = LittleEndian::read_f32(&mmap[36 + idx * 4 .. 40 + idx * 4]);
        }

        let vol_offset = 48;
        for idx in 0..16 {
            category_volumes[idx] = LittleEndian::read_f32(&mmap[vol_offset + idx * 4 .. vol_offset + (idx + 1) * 4]);
        }

        let engine_flags = LittleEndian::read_u32(&mmap[112..116]);

        compiler_fence(Ordering::Release);
        let ver_check = LittleEndian::read_u32(&mmap[OFFSET_VER .. OFFSET_VER + 4]);

        if ver == ver_check {
            if listener_pos != last_pos
                || listener_fwd != last_fwd
                || listener_up != last_up
                || category_volumes != last_category_volumes
                || engine_flags != last_engine_flags
            {
                let _ = tx_cmd.send(AudioCommand::UpdateListener {
                    pos: listener_pos,
                    fwd: listener_fwd,
                    up: listener_up,
                    category_volumes,
                    engine_flags,
                });
                last_pos = listener_pos;
                last_fwd = listener_fwd;
                last_up = listener_up;
                last_category_volumes = category_volumes;
                last_engine_flags = engine_flags;
            }
        }

        // Check if voxel grid changed
        let voxel_version = LittleEndian::read_u32(&mmap[OFFSET_VOXEL_GRID_VERSION .. OFFSET_VOXEL_GRID_VERSION + 4]);
        if voxel_version != local_voxel_version {
            local_voxel_version = voxel_version;

            let cx = LittleEndian::read_i32(&mmap[OFFSET_CENTER_X .. OFFSET_CENTER_X + 4]);
            let cy = LittleEndian::read_i32(&mmap[OFFSET_CENTER_Y .. OFFSET_CENTER_Y + 4]);
            let cz = LittleEndian::read_i32(&mmap[OFFSET_CENTER_Z .. OFFSET_CENTER_Z + 4]);

            // Read raw bytes representing the grid from mapped memory
            let mut voxel_bytes = [0u8; 32768];
            voxel_bytes.copy_from_slice(&mmap[OFFSET_VOXEL_GRID .. OFFSET_VOXEL_GRID + 32768]);

            // Spawn CPU-heavy geometry processing off the control thread
            let app_state_task = Arc::clone(&app_state);
            tokio::task::spawn_blocking(move || {
                crate::phonon::rebuild_acoustic_mesh(
                    app_state_task.scene,
                    app_state_task.context,
                    &voxel_bytes,
                    [cx, cy, cz]
                );
            });
        }

        let dev_seq = LittleEndian::read_u32(&mmap[OFFSET_DEV_SEQ .. OFFSET_DEV_SEQ + 4]);
        if dev_seq != last_dev_seq {
            let mut name_bytes = vec![0u8; 128];
            name_bytes.copy_from_slice(&mmap[OFFSET_DEV_NAME .. OFFSET_DEV_NAME + 128]);

            let len = name_bytes.iter().position(|&x| x == 0).unwrap_or(128);
            let dev_name = String::from_utf8_lossy(&name_bytes[..len]).into_owned();

            let _ = tx_cmd.send(AudioCommand::ChangeDevice { name: dev_name });
            last_dev_seq = dev_seq;
        }

        while local_read_seq < java_write_seq {
            let slot_index = (local_read_seq as usize) % QUEUE_CAPACITY;
            let offset = HEADER_SIZE + (slot_index * SLOT_SIZE);

            let opcode = LittleEndian::read_u32(&mmap[offset .. offset + 4]);

            if opcode == 0 { // OP_PLAY
                let uid = LittleEndian::read_u32(&mmap[offset + 4 .. offset + 8]);
                let x = LittleEndian::read_f32(&mmap[offset + 8 .. offset + 12]);
                let y = LittleEndian::read_f32(&mmap[offset + 12 .. offset + 16]);
                let z = LittleEndian::read_f32(&mmap[offset + 16 .. offset + 20]);
                let volume = LittleEndian::read_f32(&mmap[offset + 20 .. offset + 24]);
                let pitch = LittleEndian::read_f32(&mmap[offset + 24 .. offset + 28]);
                let asset_hash = LittleEndian::read_u32(&mmap[offset + 28 .. offset + 32]);

                let is_relative = mmap[offset + 32] != 0;
                let is_spatial = mmap[offset + 33] != 0;
                let category_id = mmap[offset + 34] as usize;

                let _ = tx_cmd.send(AudioCommand::PlaySound {
                    uid,
                    pos: [x, y, z],
                    volume,
                    pitch,
                    asset_hash,
                    is_relative,
                    is_spatial,
                    category_id,
                });
            } else if opcode == 1 { // OP_STOP
                let uid = LittleEndian::read_u32(&mmap[offset + 4 .. offset + 8]);
                let _ = tx_cmd.send(AudioCommand::StopSound { uid });
            } else if opcode == 2 { // OP_STOP_ALL
                let _ = tx_cmd.send(AudioCommand::StopAllSounds);
            }

            local_read_seq += 1;
            LittleEndian::write_u32(&mut mmap[OFFSET_RUST_READ_SEQ .. OFFSET_RUST_READ_SEQ + 4], local_read_seq);
        }

        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
}

async fn handle_client_stream<S>(mut stream: S, tx_cmd: Sender<AudioCommand>)
where
    S: AsyncReadExt + Unpin
{
    let mut buffer = vec![0u8; 65536];
    let mut pending_data = Vec::new();

    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => {
                println!("[Rust Daemon] Signal channel disconnected.");
                break;
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

                    if opcode == 1 { // OP_ASSET_LOAD
                        println!("[Rust Daemon] Streamed asset received for hash: {}", hash);

                        let tx_cmd_task = tx_cmd.clone();
                        tokio::task::spawn_blocking(move || {
                            match asset::decode_ogg_in_memory(payload) {
                                Ok(pcm_asset) => {
                                    let _ = tx_cmd_task.send(AudioCommand::LoadAsset {
                                        hash,
                                        asset: pcm_asset,
                                    });
                                }
                                Err(e) => {
                                    eprintln!("[Rust Daemon] OGG memory-decode failure: {:?}", e);
                                }
                            }
                        });
                    }
                }
            }
            Err(e) => {
                eprintln!("[Rust Daemon] IPC read error: {:?}", e);
                break;
            }
        }
    }
}