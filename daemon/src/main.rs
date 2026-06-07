mod ipc;
mod audio;
mod steam;
mod asset;

use std::env;
use std::sync::Arc;
use std::thread;
use crossbeam_channel::unbounded;
use cpal::traits::{DeviceTrait, HostTrait}; // Removed StreamTrait

#[allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]
pub mod steam_audio {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub struct AppState {
    pub context: steam_audio::IPLContext,
    pub hrtf: steam_audio::IPLHRTF,
}

unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

#[tokio::main]
async fn main() {
    thread::spawn(|| {
        use std::io::Read;
        let mut buffer = [0; 1];
        let _ = std::io::stdin().read(&mut buffer);
        println!("[Rust Daemon] Parent JVM disconnected. Shutting down...");
        std::process::exit(0);
    });

    println!("[Rust Daemon] Booting Steam Audio Engine...");

    let mut context: steam_audio::IPLContext = std::ptr::null_mut();
    let mut ctx_settings = steam_audio::IPLContextSettings {
        version: (steam_audio::STEAMAUDIO_VERSION_MAJOR << 16)
            | (steam_audio::STEAMAUDIO_VERSION_MINOR << 8)
            | steam_audio::STEAMAUDIO_VERSION_PATCH,
        logCallback: None,
        allocateCallback: None,
        freeCallback: None,
        simdLevel: steam_audio::IPLSIMDLevel_IPL_SIMDLEVEL_AVX2,
        flags: 0,
    };

    unsafe {
        let status = steam_audio::iplContextCreate(&mut ctx_settings, &mut context);
        if status != steam_audio::IPLerror_IPL_STATUS_SUCCESS {
            panic!("Failed to create Steam Audio Context! Error Code: {}", status);
        }
    }
    println!("[Rust Daemon] Steam Audio IPLContext created successfully!");

    let mut hrtf: steam_audio::IPLHRTF = std::ptr::null_mut();
    let mut hrtf_settings = steam_audio::IPLHRTFSettings {
        type_: steam_audio::IPLHRTFType_IPL_HRTFTYPE_DEFAULT,
        sofaFileName: std::ptr::null_mut(),
        volume: 1.0,
        normType: steam_audio::IPLHRTFNormType_IPL_HRTFNORMTYPE_NONE,
        sofaData: std::ptr::null_mut(),
        sofaDataSize: 0,
    };

    let host = cpal::default_host();
    let device = host.default_output_device().expect("No output device found!");
    let config = device.default_output_config().expect("Failed to get config!");

    let mut audio_settings = steam_audio::IPLAudioSettings {
        samplingRate: 48000,
        frameSize: 512,
    };

    unsafe {
        let status = steam_audio::iplHRTFCreate(context, &mut audio_settings, &mut hrtf_settings, &mut hrtf);
        if status != steam_audio::IPLerror_IPL_STATUS_SUCCESS {
            panic!("Failed to create Steam Audio HRTF! Error Code: {}", status);
        }
    }
    println!("[Rust Daemon] Steam Audio HRTF created successfully!");

    let app_state = Arc::new(AppState { context, hrtf });
    let (tx_cmd, rx_cmd) = unbounded::<audio::AudioCommand>();

    let app_state_audio = Arc::clone(&app_state);
    thread::spawn(move || {
        audio::run_audio_thread(device, config, app_state_audio, rx_cmd);
    });

    let app_state_ipc = Arc::clone(&app_state);
    let shm_path = env::var("DECIBEL_SHM_PATH").expect("DECIBEL_SHM_PATH not set!");

    ipc::run_ipc_loop(shm_path, app_state_ipc, tx_cmd).await;
}