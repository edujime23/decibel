use crate::steam_audio::*;
use std::ptr;
use std::sync::{Arc, Mutex};
use crate::AppState;

pub struct SteamDirectEffect { pub effect: IPLDirectEffect }
unsafe impl Send for SteamDirectEffect {}
unsafe impl Sync for SteamDirectEffect {}
impl SteamDirectEffect {
    pub fn new(context: IPLContext, sample_rate: i32, frame_size: i32) -> Option<Self> {
        let mut effect: IPLDirectEffect = ptr::null_mut();
        let mut audio_settings = IPLAudioSettings { samplingRate: sample_rate, frameSize: frame_size };
        let mut effect_settings = IPLDirectEffectSettings { numChannels: 1 };
        if unsafe { iplDirectEffectCreate(context, &mut audio_settings, &mut effect_settings, &mut effect) } != IPLerror_IPL_STATUS_SUCCESS { return None; }
        Some(Self { effect })
    }
    pub fn apply(&mut self, input_channel: &[f32], sim_outputs: &IPLSimulationOutputs, flags_bitmask: u32, out_channel: &mut [f32]) {
        if self.effect.is_null() { return; }
        let mut input_data = input_channel.as_ptr() as *mut f32;
        let mut input_buffer = IPLAudioBuffer { numChannels: 1, numSamples: input_channel.len() as i32, data: &mut input_data };
        let mut out_data = out_channel.as_mut_ptr();
        let mut out_buffer = IPLAudioBuffer { numChannels: 1, numSamples: out_channel.len() as i32, data: &mut out_data };
        let mut effect_flags = IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYDISTANCEATTENUATION | IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYAIRABSORPTION;
        if (flags_bitmask & (1 << 2)) != 0 { effect_flags |= IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYOCCLUSION; }
        if (flags_bitmask & (1 << 3)) != 0 { effect_flags |= IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYTRANSMISSION; }
        let mut params = sim_outputs.direct; params.flags = effect_flags;
        unsafe { iplDirectEffectApply(self.effect, &mut params, &mut input_buffer, &mut out_buffer); }
    }
}
impl Drop for SteamDirectEffect { fn drop(&mut self) { if !self.effect.is_null() { unsafe { iplDirectEffectRelease(&mut self.effect); } } } }

pub struct SteamBinauralEffect { pub effect: IPLBinauralEffect }
unsafe impl Send for SteamBinauralEffect {}
unsafe impl Sync for SteamBinauralEffect {}
impl SteamBinauralEffect {
    pub fn new(context: IPLContext, sample_rate: i32, frame_size: i32, hrtf: IPLHRTF) -> Option<Self> {
        let mut effect: IPLBinauralEffect = ptr::null_mut();
        let mut audio_settings = IPLAudioSettings { samplingRate: sample_rate, frameSize: frame_size };
        let mut effect_settings = IPLBinauralEffectSettings { hrtf };
        if unsafe { iplBinauralEffectCreate(context, &mut audio_settings, &mut effect_settings, &mut effect) } != IPLerror_IPL_STATUS_SUCCESS { return None; }
        Some(Self { effect })
    }
    pub fn apply(&mut self, input_channel: &[f32], direction: IPLVector3, hrtf: IPLHRTF, out_stereo_l: &mut [f32], out_stereo_r: &mut [f32]) {
        if self.effect.is_null() { return; }
        let mut input_data = input_channel.as_ptr() as *mut f32;
        let mut input_buffer = IPLAudioBuffer { numChannels: 1, numSamples: input_channel.len() as i32, data: &mut input_data };
        let mut out_channels = [out_stereo_l.as_mut_ptr(), out_stereo_r.as_mut_ptr()];
        let mut out_buffer = IPLAudioBuffer { numChannels: 2, numSamples: out_stereo_l.len() as i32, data: out_channels.as_mut_ptr() };
        let mut params = IPLBinauralEffectParams { direction, interpolation: IPLHRTFInterpolation_IPL_HRTFINTERPOLATION_NEAREST, spatialBlend: 1.0, hrtf, peakDelays: ptr::null_mut() };
        unsafe { iplBinauralEffectApply(self.effect, &mut params, &mut input_buffer, &mut out_buffer); }
    }
}
impl Drop for SteamBinauralEffect { fn drop(&mut self) { if !self.effect.is_null() { unsafe { iplBinauralEffectRelease(&mut self.effect); } } } }

// CRITICAL STRUCT: Prevents Steam Audio from reading garbage memory.
pub struct SceneData {
    pub scene: IPLScene,
    pub static_mesh: IPLStaticMesh,
    pub vertices: Vec<IPLVector3>,
    pub triangles: Vec<IPLTriangle>,
    pub material_indices: Vec<i32>,
    pub materials: Vec<IPLMaterial>,
}
unsafe impl Send for SceneData {}
unsafe impl Sync for SceneData {}

impl Drop for SceneData {
    fn drop(&mut self) {
        unsafe {
            if !self.static_mesh.is_null() {
                iplStaticMeshRemove(self.static_mesh, self.scene);
                iplStaticMeshRelease(&mut self.static_mesh);
            }
            if !self.scene.is_null() { iplSceneRelease(&mut self.scene); }
        }
    }
}

pub struct SteamSource { pub source: IPLSource, pub simulator: IPLSimulator, pub mutex: Arc<Mutex<()>> }
unsafe impl Send for SteamSource {}
unsafe impl Sync for SteamSource {}
impl Drop for SteamSource {
    fn drop(&mut self) {
        if !self.source.is_null() {
            let _guard = self.mutex.lock().unwrap_or_else(|e| e.into_inner());
            unsafe {
                iplSourceRemove(self.source, self.simulator);
                iplSimulatorCommit(self.simulator); // CRITICAL FIX: MUST COMMIT REMOVAL TO PREVENT SEGFAULT
                iplSourceRelease(&mut self.source);
            }
        }
    }
}

pub fn create_ipl_source(simulator: IPLSimulator, pos: [f32; 3], mutex: Arc<Mutex<()>>) -> Option<SteamSource> {
    let mut source: IPLSource = ptr::null_mut();
    let mut settings = IPLSourceSettings { flags: IPLSimulationFlags_IPL_SIMULATIONFLAGS_DIRECT | IPLSimulationFlags_IPL_SIMULATIONFLAGS_REFLECTIONS };
    unsafe {
        if iplSourceCreate(simulator, &mut settings, &mut source) != IPLerror_IPL_STATUS_SUCCESS { return None; }
        iplSourceAdd(source, simulator);
    }
    let mut source_inputs: IPLSimulationInputs = unsafe { std::mem::zeroed() };
    source_inputs.flags = IPLSimulationFlags_IPL_SIMULATIONFLAGS_DIRECT | IPLSimulationFlags_IPL_SIMULATIONFLAGS_REFLECTIONS;
    source_inputs.directFlags = IPLDirectSimulationFlags_IPL_DIRECTSIMULATIONFLAGS_OCCLUSION | IPLDirectSimulationFlags_IPL_DIRECTSIMULATIONFLAGS_TRANSMISSION;
    source_inputs.source.origin = IPLVector3 { x: pos[0], y: pos[1], z: pos[2] };
    source_inputs.source.right = IPLVector3 { x: 1.0, y: 0.0, z: 0.0 };
    source_inputs.source.up = IPLVector3 { x: 0.0, y: 1.0, z: 0.0 };
    source_inputs.source.ahead = IPLVector3 { x: 0.0, y: 0.0, z: 1.0 };
    source_inputs.distanceAttenuationModel.type_ = IPLDistanceAttenuationModelType_IPL_DISTANCEATTENUATIONTYPE_INVERSEDISTANCE;
    source_inputs.distanceAttenuationModel.minDistance = 1.0;
    unsafe { iplSourceSetInputs(source, IPLSimulationFlags_IPL_SIMULATIONFLAGS_DIRECT | IPLSimulationFlags_IPL_SIMULATIONFLAGS_REFLECTIONS, &mut source_inputs); }
    Some(SteamSource { source, simulator, mutex })
}

const ACOUSTIC_MATERIALS: [IPLMaterial; 5] = [
    IPLMaterial { absorption: [0.0; 3], scattering: 0.0, transmission: [1.0; 3] },
    IPLMaterial { absorption: [0.05, 0.05, 0.05], scattering: 0.1, transmission: [0.01, 0.01, 0.01] },
    IPLMaterial { absorption: [0.10, 0.20, 0.30], scattering: 0.5, transmission: [0.10, 0.05, 0.02] },
    IPLMaterial { absorption: [0.60, 0.80, 0.95], scattering: 0.8, transmission: [0.20, 0.05, 0.01] },
    IPLMaterial { absorption: [0.02, 0.02, 0.05], scattering: 0.05, transmission: [0.60, 0.40, 0.20] },
];

fn make_vector(d: usize, u: usize, v: usize, cd: f32, cu: f32, cv: f32, center: [i32; 3]) -> IPLVector3 {
    let mut coords = [0.0f32; 3];
    coords[d] = cd + center[d] as f32; coords[u] = cu + center[u] as f32; coords[v] = cv + center[v] as f32;
    IPLVector3 { x: coords[0], y: coords[1], z: coords[2] }
}

pub fn rebuild_acoustic_mesh(app_state: Arc<AppState>, voxels: &[u8], min_bounds: [i32; 3]) {
    let mut vertices: Vec<IPLVector3> = Vec::new();
    let mut triangles: Vec<IPLTriangle> = Vec::new();
    let mut material_indices: Vec<i32> = Vec::new();

    for d in 0..3 {
        let u = (d + 1) % 3; let v = (d + 2) % 3;
        for slice in 0..64 {
            for side in 0..2 {
                let mut mask = [[0u8; 64]; 64];
                for u_coord in 0..64 {
                    for v_coord in 0..64 {
                        let mut coords = [0; 3];
                        coords[d] = slice; coords[u] = u_coord; coords[v] = v_coord;
                        let current_val = voxels[(coords[0] * 4096) + (coords[1] * 64) + coords[2]];

                        if current_val > 0 {
                            let mut n_coords = coords;
                            let has_neighbor = if side == 0 {
                                if slice > 0 { n_coords[d] = slice - 1; voxels[(n_coords[0] * 4096) + (n_coords[1] * 64) + n_coords[2]] > 0 } else { false }
                            } else {
                                if slice < 63 { n_coords[d] = slice + 1; voxels[(n_coords[0] * 4096) + (n_coords[1] * 64) + n_coords[2]] > 0 } else { false }
                            };
                            if !has_neighbor { mask[u_coord][v_coord] = current_val; }
                        }
                    }
                }

                let mut visited = [[false; 64]; 64];
                for u_s in 0..64 {
                    for v_s in 0..64 {
                        let m = mask[u_s][v_s];
                        if m == 0 || visited[u_s][v_s] { continue; }

                        let mut w = 1;
                        while u_s + w < 64 && mask[u_s + w][v_s] == m && !visited[u_s + w][v_s] { w += 1; }
                        let mut h = 1;
                        'outer: while v_s + h < 64 {
                            for k in 0..w { if mask[u_s + k][v_s + h] != m || visited[u_s + k][v_s + h] { break 'outer; } }
                            h += 1;
                        }
                        for ku in 0..w { for kv in 0..h { visited[u_s + ku][v_s + kv] = true; } }

                        let cd = if side == 0 { slice as f32 } else { (slice + 1) as f32 };
                        let base_idx = vertices.len() as i32;

                        vertices.push(make_vector(d, u, v, cd, u_s as f32, v_s as f32, min_bounds));
                        vertices.push(make_vector(d, u, v, cd, (u_s + w) as f32, v_s as f32, min_bounds));
                        vertices.push(make_vector(d, u, v, cd, (u_s + w) as f32, (v_s + h) as f32, min_bounds));
                        vertices.push(make_vector(d, u, v, cd, u_s as f32, (v_s + h) as f32, min_bounds));

                        triangles.push(IPLTriangle { indices: [base_idx, base_idx + 3, base_idx + 2] });
                        triangles.push(IPLTriangle { indices: [base_idx, base_idx + 2, base_idx + 1] });

                        let mat_idx = (m as i32 - 1).clamp(0, 4);
                        material_indices.push(mat_idx); material_indices.push(mat_idx);
                    }
                }
            }
        }
    }

    if vertices.is_empty() { return; }

    unsafe {
        let mut scene_settings = IPLSceneSettings {
            type_: IPLSceneType_IPL_SCENETYPE_DEFAULT,
            closestHitCallback: None, anyHitCallback: None, batchedClosestHitCallback: None, batchedAnyHitCallback: None,
            userData: ptr::null_mut(), embreeDevice: ptr::null_mut(), radeonRaysDevice: ptr::null_mut(),
        };
        let mut new_scene: IPLScene = ptr::null_mut();
        if iplSceneCreate(app_state.context, &mut scene_settings, &mut new_scene) != IPLerror_IPL_STATUS_SUCCESS { return; }

        let mut static_mesh: IPLStaticMesh = ptr::null_mut();
        let mut materials_array = ACOUSTIC_MATERIALS.to_vec();

        let mut mesh_settings = IPLStaticMeshSettings {
            numVertices: vertices.len() as i32, numTriangles: triangles.len() as i32,
            vertices: vertices.as_mut_ptr(), triangles: triangles.as_mut_ptr(),
            materialIndices: material_indices.as_mut_ptr(), materials: materials_array.as_mut_ptr(), numMaterials: materials_array.len() as i32,
        };

        if iplStaticMeshCreate(new_scene, &mut mesh_settings, &mut static_mesh) == IPLerror_IPL_STATUS_SUCCESS {
            iplStaticMeshAdd(static_mesh, new_scene);
            iplSceneCommit(new_scene);

            let new_scene_data = SceneData {
                scene: new_scene,
                static_mesh,
                vertices,
                triangles,
                material_indices,
                materials: materials_array,
            };

            {
                let _guard = app_state.simulator_mutex.lock().unwrap_or_else(|e| e.into_inner());
                iplSimulatorSetScene(app_state.simulator, new_scene);
                iplSimulatorCommit(app_state.simulator); // CRITICAL FIX: MUST COMMIT SCENE

                // Store in AppState to ensure memory isn't dropped! Old scene automatically cleans itself up safely.
                let mut current_scene_guard = app_state.current_scene.lock().unwrap();
                *current_scene_guard = Some(new_scene_data);
            }
        } else {
            iplSceneRelease(&mut new_scene);
        }
    }
}