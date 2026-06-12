mod ipc;
mod audio;
mod phonon;
mod asset;

use std::env;
use std::sync::{Arc, Mutex};
use std::thread;
use crossbeam_channel::unbounded;
use cpal::traits::{DeviceTrait, HostTrait};

#[allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]
pub mod steam_audio {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub struct AppState {
    pub context: steam_audio::IPLContext,
    pub hrtf: steam_audio::IPLHRTF,
    pub simulator: steam_audio::IPLSimulator,
    pub simulator_mutex: Arc<Mutex<()>>,
    pub current_scene: Mutex<Option<crate::phonon::SceneData>>,
}

unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

#[tokio::main]
async fn main() {
    println!("[Rust Daemon] Booting Steam Audio Engine...");

    let mut context: steam_audio::IPLContext = std::ptr::null_mut();
    let mut ctx_settings = steam_audio::IPLContextSettings {
        version: (steam_audio::STEAMAUDIO_VERSION_MAJOR << 16) | (steam_audio::STEAMAUDIO_VERSION_MINOR << 8) | steam_audio::STEAMAUDIO_VERSION_PATCH,
        logCallback: None, allocateCallback: None, freeCallback: None,
        simdLevel: steam_audio::IPLSIMDLevel_IPL_SIMDLEVEL_AVX2, flags: 0,
    };

    if unsafe { steam_audio::iplContextCreate(&mut ctx_settings, &mut context) } != steam_audio::IPLerror_IPL_STATUS_SUCCESS {
        eprintln!("[Rust Daemon] FATAL: Failed to initialize Steam Audio Context."); std::process::exit(1);
    }

    let mut hrtf: steam_audio::IPLHRTF = std::ptr::null_mut();
    let mut hrtf_settings = steam_audio::IPLHRTFSettings {
        type_: steam_audio::IPLHRTFType_IPL_HRTFTYPE_DEFAULT,
        sofaFileName: std::ptr::null_mut(), volume: 1.0,
        normType: steam_audio::IPLHRTFNormType_IPL_HRTFNORMTYPE_NONE, sofaData: std::ptr::null_mut(), sofaDataSize: 0,
    };

    let mut audio_settings = steam_audio::IPLAudioSettings { samplingRate: 48000, frameSize: 512 };
    if unsafe { steam_audio::iplHRTFCreate(context, &mut audio_settings, &mut hrtf_settings, &mut hrtf) } != steam_audio::IPLerror_IPL_STATUS_SUCCESS {
        eprintln!("[Rust Daemon] FATAL: Failed to initialize HRTF."); std::process::exit(1);
    }

    let mut simulator: steam_audio::IPLSimulator = std::ptr::null_mut();
    let mut sim_settings: steam_audio::IPLSimulationSettings = unsafe { std::mem::zeroed() };
    sim_settings.flags = steam_audio::IPLSimulationFlags_IPL_SIMULATIONFLAGS_DIRECT | steam_audio::IPLSimulationFlags_IPL_SIMULATIONFLAGS_REFLECTIONS;
    sim_settings.sceneType = steam_audio::IPLSceneType_IPL_SCENETYPE_DEFAULT;
    sim_settings.reflectionType = steam_audio::IPLReflectionEffectType_IPL_REFLECTIONEFFECTTYPE_CONVOLUTION;
    sim_settings.maxNumOcclusionSamples = 32; sim_settings.maxNumRays = 4096; sim_settings.numDiffuseSamples = 1024;
    sim_settings.maxDuration = 2.0; sim_settings.maxOrder = 1; sim_settings.maxNumSources = 256;
    sim_settings.numThreads = 2; sim_settings.samplingRate = 48000; sim_settings.frameSize = 512;

    if unsafe { steam_audio::iplSimulatorCreate(context, &mut sim_settings, &mut simulator) } != steam_audio::IPLerror_IPL_STATUS_SUCCESS {
        eprintln!("[Rust Daemon] FATAL: Failed to initialize Steam Audio Simulator."); std::process::exit(1);
    }

    // CRITICAL FIX: Initialize simulator inputs immediately so it doesn't crash on the first 16ms tick
    let mut inputs: steam_audio::IPLSimulationSharedInputs = unsafe { std::mem::zeroed() };
    inputs.listener.origin = steam_audio::IPLVector3 { x: 0.0, y: 0.0, z: 0.0 };
    inputs.listener.ahead = steam_audio::IPLVector3 { x: 0.0, y: 0.0, z: 1.0 };
    inputs.listener.up = steam_audio::IPLVector3 { x: 0.0, y: 1.0, z: 0.0 };
    inputs.listener.right = steam_audio::IPLVector3 { x: 1.0, y: 0.0, z: 0.0 };

    unsafe {
        steam_audio::iplSimulatorSetSharedInputs(simulator, steam_audio::IPLSimulationFlags_IPL_SIMULATIONFLAGS_DIRECT | steam_audio::IPLSimulationFlags_IPL_SIMULATIONFLAGS_REFLECTIONS, &mut inputs);
        steam_audio::iplSimulatorCommit(simulator);
    }

    let app_state = Arc::new(AppState {
        context, hrtf, simulator,
        simulator_mutex: Arc::new(Mutex::new(())),
        current_scene: Mutex::new(None),
    });

    let (tx_cmd, rx_cmd) = unbounded::<audio::AudioCommand>();
    let app_state_audio = Arc::clone(&app_state);

    thread::spawn(move || {
        let host = cpal::default_host();
        let device = host.default_output_device().expect("No audio output device found.");
        let config = device.default_output_config().expect("Failed to get CPAL config");
        audio::engine::run_audio_thread(device, config, app_state_audio, rx_cmd);
    });

    let app_state_ipc = Arc::clone(&app_state);
    let shm_path = env::var("DECIBEL_SHM_PATH").expect("DECIBEL_SHM_PATH not set!");

    ipc::shm::run_ipc_loop(shm_path, app_state_ipc, tx_cmd).await;
}