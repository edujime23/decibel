use std::sync::Arc;
use std::path::Path;
use std::fs::OpenOptions;
use std::sync::atomic::{compiler_fence, Ordering};
use std::time::Instant;
use memmap2::MmapMut;
use crossbeam_channel::Sender;

use crate::AppState;
use crate::audio::AudioCommand;
use crate::ipc::signal::handle_client_stream;

const QUEUE_CAPACITY: usize = 1024;
const SLOT_SIZE: usize = 64;

const OFFSET_JAVA_WRITE_SEQ: usize      = 0;
const OFFSET_RUST_READ_SEQ: usize       = 4;
const OFFSET_HEARTBEAT: usize           = 8;
const OFFSET_VER: usize                 = 12;
const OFFSET_DEV_SEQ: usize             = 16;
const OFFSET_VOXEL_GRID_VERSION: usize  = 20;
const OFFSET_START_X: usize             = 24;
const OFFSET_START_Y: usize             = 28;
const OFFSET_START_Z: usize             = 32;
const OFFSET_FLAGS: usize               = 36;

const OFFSET_LISTENER_POS: usize        = 40;
const OFFSET_LISTENER_FWD: usize        = 52;
const OFFSET_LISTENER_UP: usize         = 64;
const OFFSET_CATEGORY_VOLUMES: usize    = 76;
const OFFSET_DEV_NAME: usize            = 140;

const HEADER_SIZE: usize                = 512;
const RING_BUFFER_SIZE: usize           = QUEUE_CAPACITY * SLOT_SIZE;
const OFFSET_VOXEL_GRID: usize          = HEADER_SIZE + RING_BUFFER_SIZE;
const REQUIRED_SHM_SIZE: usize          = OFFSET_VOXEL_GRID + 262144; // 328,192 bytes

unsafe fn read_volatile_u32(mmap: &MmapMut, offset: usize) -> u32 {
    let ptr = mmap.as_ptr().add(offset) as *const u32;
    std::ptr::read_volatile(ptr)
}

unsafe fn write_volatile_u32(mmap: &mut MmapMut, offset: usize, val: u32) {
    let ptr = mmap.as_mut_ptr().add(offset) as *mut u32;
    std::ptr::write_volatile(ptr, val);
}

pub async fn run_ipc_loop(shm_path: String, app_state: Arc<AppState>, tx_cmd: Sender<AudioCommand>) {
    let shm_file_path = Path::new(&shm_path);
    let _tmp_dir = shm_file_path.parent().unwrap();

    let mut mmap = loop {
        if let Ok(file) = OpenOptions::new().read(true).write(true).open(shm_file_path) {
            if let Ok(metadata) = file.metadata() {
                if metadata.len() >= REQUIRED_SHM_SIZE as u64 {
                    break unsafe { MmapMut::map_mut(&file).unwrap_or_else(|_| std::process::exit(1)) };
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };

    let tx_cmd_clone = tx_cmd.clone();
    let app_state_signal = Arc::clone(&app_state);

    #[cfg(unix)]
    let tmp_dir_clone = _tmp_dir.to_path_buf();

    tokio::spawn(async move {
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            let pipe_name = r"\\.\pipe\decibel_engine";
            loop {
                let mut server = match ServerOptions::new().create(pipe_name) {
                    Ok(s) => s,
                    Err(e) => { eprintln!("Pipe error: {}", e); tokio::time::sleep(std::time::Duration::from_secs(1)).await; continue; }
                };
                if server.connect().await.is_ok() {
                    handle_client_stream(server, Arc::clone(&app_state_signal), tx_cmd_clone.clone()).await;
                }
            }
        }

        #[cfg(unix)]
        {
            use tokio::net::UnixListener;
            let socket_path = tmp_dir_clone.join("decibel_engine.sock");
            if socket_path.exists() { let _ = std::fs::remove_file(&socket_path); }
            let listener = UnixListener::bind(&socket_path).unwrap();
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    handle_client_stream(stream, Arc::clone(&app_state_signal), tx_cmd_clone.clone()).await;
                }
            }
        }
    });

    let mut local_read_seq = 0;
    let mut local_voxel_version = 0;
    let mut last_pos = [0.0f32; 3];
    let mut last_fwd = [0.0f32; 3];
    let mut last_up = [0.0f32; 3];
    let mut last_category_volumes = [1.0f32; 16];
    let mut last_engine_flags = 0u32;
    let mut last_dev_seq = 0u32;

    let mut last_heartbeat_val = 0u32;
    let mut last_heartbeat_time = Instant::now();
    let mut last_sim_time = Instant::now();

    loop {
        let current_heartbeat = unsafe { read_volatile_u32(&mmap, OFFSET_HEARTBEAT) };
        if current_heartbeat != last_heartbeat_val {
            last_heartbeat_val = current_heartbeat;
            last_heartbeat_time = Instant::now();
        } else if current_heartbeat > 0 && last_heartbeat_time.elapsed().as_secs() > 15 {
            println!("[Rust Daemon] Heartbeat flatlined. JVM terminated. Exiting.");
            std::process::exit(0);
        }

        let java_write_seq = unsafe { read_volatile_u32(&mmap, OFFSET_JAVA_WRITE_SEQ) };
        if java_write_seq.wrapping_sub(local_read_seq) > QUEUE_CAPACITY as u32 { local_read_seq = java_write_seq; }

        let mut listener_pos = [0.0f32; 3];
        let mut listener_fwd = [0.0f32; 3];
        let mut listener_up = [0.0f32; 3];
        let mut category_volumes = [1.0f32; 16];

        let mut ver = unsafe { read_volatile_u32(&mmap, OFFSET_VER) };
        let mut attempts = 0;
        while ver % 2 != 0 && attempts < 100 { std::hint::spin_loop(); ver = unsafe { read_volatile_u32(&mmap, OFFSET_VER) }; attempts += 1; }

        compiler_fence(Ordering::Acquire);
        for idx in 0..3 {
            listener_pos[idx] = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(OFFSET_LISTENER_POS + idx * 4) as *const f32) };
            listener_fwd[idx] = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(OFFSET_LISTENER_FWD + idx * 4) as *const f32) };
            listener_up[idx] = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(OFFSET_LISTENER_UP + idx * 4) as *const f32) };
        }
        for idx in 0..16 { category_volumes[idx] = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(OFFSET_CATEGORY_VOLUMES + idx * 4) as *const f32) }; }
        let engine_flags = unsafe { read_volatile_u32(&mmap, OFFSET_FLAGS) };
        compiler_fence(Ordering::Release);

        if ver == unsafe { read_volatile_u32(&mmap, OFFSET_VER) } {
            if listener_pos != last_pos || listener_fwd != last_fwd || listener_up != last_up || category_volumes != last_category_volumes || engine_flags != last_engine_flags {
                let _ = tx_cmd.send(AudioCommand::UpdateListener { pos: listener_pos, fwd: listener_fwd, up: listener_up, category_volumes, engine_flags });

                let mut inputs: crate::steam_audio::IPLSimulationSharedInputs = unsafe { std::mem::zeroed() };
                inputs.listener.origin = crate::steam_audio::IPLVector3 { x: listener_pos[0], y: listener_pos[1], z: listener_pos[2] };
                inputs.listener.ahead = crate::steam_audio::IPLVector3 { x: listener_fwd[0], y: listener_fwd[1], z: listener_fwd[2] };
                inputs.listener.up = crate::steam_audio::IPLVector3 { x: listener_up[0], y: listener_up[1], z: listener_up[2] };
                inputs.listener.right = crate::steam_audio::IPLVector3 {
                    x: listener_up[1]*listener_fwd[2] - listener_up[2]*listener_fwd[1],
                    y: listener_up[2]*listener_fwd[0] - listener_up[0]*listener_fwd[2],
                    z: listener_up[0]*listener_fwd[1] - listener_up[1]*listener_fwd[0],
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

                last_pos = listener_pos; last_fwd = listener_fwd; last_up = listener_up; last_category_volumes = category_volumes; last_engine_flags = engine_flags;
            }
        }

        let voxel_version = unsafe { read_volatile_u32(&mmap, OFFSET_VOXEL_GRID_VERSION) };
        if voxel_version != local_voxel_version {
            local_voxel_version = voxel_version;
            let sx = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(OFFSET_START_X) as *const i32) };
            let sy = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(OFFSET_START_Y) as *const i32) };
            let sz = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(OFFSET_START_Z) as *const i32) };

            let mut voxel_bytes = vec![0u8; 262144].into_boxed_slice();
            unsafe { std::ptr::copy_nonoverlapping(mmap.as_ptr().add(OFFSET_VOXEL_GRID), voxel_bytes.as_mut_ptr(), 262144); }

            crate::phonon::rebuild_acoustic_mesh(Arc::clone(&app_state), &*voxel_bytes, [sx, sy, sz]);
        }

        let dev_seq = unsafe { read_volatile_u32(&mmap, OFFSET_DEV_SEQ) };
        if dev_seq != last_dev_seq {
            let mut name_bytes = vec![0u8; 128];
            unsafe { std::ptr::copy_nonoverlapping(mmap.as_ptr().add(OFFSET_DEV_NAME), name_bytes.as_mut_ptr(), 128); }
            let len = name_bytes.iter().position(|&x| x == 0).unwrap_or(128);
            let dev_name = String::from_utf8_lossy(&name_bytes[..len]).into_owned();
            let _ = tx_cmd.send(AudioCommand::ChangeDevice { name: dev_name });
            last_dev_seq = dev_seq;
        }

        while local_read_seq < java_write_seq {
            let slot_index = (local_read_seq as usize) % QUEUE_CAPACITY;
                let offset = HEADER_SIZE + (slot_index * SLOT_SIZE);
                let opcode = unsafe { read_volatile_u32(&mmap, offset) };

            if opcode == 255 { break; }

            if opcode == 0 {
                let uid = unsafe { read_volatile_u32(&mmap, offset + 4) };
                let x = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(offset + 8) as *const f32) };
                let y = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(offset + 12) as *const f32) };
                let z = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(offset + 16) as *const f32) };
                let volume = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(offset + 20) as *const f32) };
                let pitch = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(offset + 24) as *const f32) };
                let asset_hash = unsafe { read_volatile_u32(&mmap, offset + 28) };
                let is_relative = mmap[offset + 32] != 0;
                let is_spatial = mmap[offset + 33] != 0;
                let category_id = mmap[offset + 34] as usize;

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

                let _ = tx_cmd.send(AudioCommand::PlaySound {
                    uid, pos: [x, y, z], volume, pitch, asset_hash, is_relative, is_spatial, category_id,
                    direct_effect, binaural_effect, ipl_source
                });
            } else if opcode == 1 {
                    let uid = unsafe { read_volatile_u32(&mmap, offset + 4) };
                    let _ = tx_cmd.send(AudioCommand::StopSound { uid });
                } else if opcode == 2 {
                    let _ = tx_cmd.send(AudioCommand::StopAllSounds);
                } else if opcode == 3 {
                    // FIX: Handle real-time coordinates update event from Shared Memory
                    let uid = unsafe { read_volatile_u32(&mmap, offset + 4) };
                    let x = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(offset + 8) as *const f32) };
                    let y = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(offset + 12) as *const f32) };
                    let z = unsafe { std::ptr::read_volatile(mmap.as_ptr().add(offset + 16) as *const f32) };

                    let _ = tx_cmd.send(AudioCommand::UpdateSoundPosition { uid, pos: [x, y, z] });
                }

                local_read_seq += 1;
                unsafe { write_volatile_u32(&mut mmap, OFFSET_RUST_READ_SEQ, local_read_seq) };
        }

        if last_sim_time.elapsed().as_millis() > 16 {
            let has_scene = app_state.current_scene.lock().unwrap().is_some();
            if has_scene {
                unsafe {
                    let _guard = app_state.simulator_mutex.lock().unwrap_or_else(|e| e.into_inner());
                    crate::steam_audio::iplSimulatorRunDirect(app_state.simulator);
                    crate::steam_audio::iplSimulatorRunReflections(app_state.simulator);
                }
            }
            last_sim_time = Instant::now();
        }

        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
}