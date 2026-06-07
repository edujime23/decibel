use crate::steam_audio::*;
use std::ptr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// FFI Thread-Safety Wrapper
#[derive(Copy, Clone)]
pub struct SendStaticMesh(pub IPLStaticMesh);
unsafe impl Send for SendStaticMesh {}
unsafe impl Sync for SendStaticMesh {}

struct SafeMesh {
    vertices: Vec<IPLVector3>,
    triangles: Vec<IPLTriangle>,
    materials: Vec<i32>,
}

pub struct EnvironmentAcoustics {
    pub t60_low: AtomicU32,
    pub t60_mid: AtomicU32,
    pub t60_high: AtomicU32,
    pub wet_mix: AtomicU32,
}

impl EnvironmentAcoustics {
    pub const fn new() -> Self {
        EnvironmentAcoustics {
            t60_low: AtomicU32::new(0x3dcccccd),  // 0.1 float bits
            t60_mid: AtomicU32::new(0x3dcccccd),  // 0.1 float bits
            t60_high: AtomicU32::new(0x3dcccccd), // 0.1 float bits
            wet_mix: AtomicU32::new(0),           // 0.0 float bits
        }
    }

    pub fn load_relaxed(&self) -> ([f32; 3], f32) {
        let low = f32::from_bits(self.t60_low.load(Ordering::Relaxed));
        let mid = f32::from_bits(self.t60_mid.load(Ordering::Relaxed));
        let high = f32::from_bits(self.t60_high.load(Ordering::Relaxed));
        let wet = f32::from_bits(self.wet_mix.load(Ordering::Relaxed));
        ([low, mid, high], wet)
    }

    pub fn store_relaxed(&self, t60: [f32; 3], wet_mix: f32) {
        self.t60_low.store(t60[0].to_bits(), Ordering::Relaxed);
        self.t60_mid.store(t60[1].to_bits(), Ordering::Relaxed);
        self.t60_high.store(t60[2].to_bits(), Ordering::Relaxed);
        self.wet_mix.store(wet_mix.to_bits(), Ordering::Relaxed);
    }
}

// Global thread-safe lock-free environment tracking
static ACTIVE_STATIC_MESH: Mutex<Option<SendStaticMesh>> = Mutex::new(None);
static ACTIVE_SAFE_MESH: Mutex<Option<SafeMesh>> = Mutex::new(None);
pub static ACTIVE_ENVIRONMENT: EnvironmentAcoustics = EnvironmentAcoustics::new();

pub static IS_SCENE_COMMITTING: AtomicBool = AtomicBool::new(false);

const ACOUSTIC_MATERIALS: [IPLMaterial; 5] = [
    IPLMaterial { absorption: [0.0; 3], scattering: 0.0, transmission: [1.0; 3] },
    IPLMaterial { absorption: [0.05, 0.05, 0.05], scattering: 0.1, transmission: [0.01, 0.01, 0.01] },
    IPLMaterial { absorption: [0.10, 0.20, 0.30], scattering: 0.5, transmission: [0.10, 0.05, 0.02] },
    IPLMaterial { absorption: [0.60, 0.80, 0.95], scattering: 0.8, transmission: [0.20, 0.05, 0.01] },
    IPLMaterial { absorption: [0.02, 0.02, 0.05], scattering: 0.05, transmission: [0.60, 0.40, 0.20] },
];

pub struct SteamDirectEffect {
    effect: IPLDirectEffect,
}

unsafe impl Send for SteamDirectEffect {}
unsafe impl Sync for SteamDirectEffect {}

impl SteamDirectEffect {
    pub fn new(context: IPLContext, sample_rate: i32, frame_size: i32) -> Self {
        let mut effect: IPLDirectEffect = ptr::null_mut();
        let mut audio_settings = IPLAudioSettings {
            samplingRate: sample_rate,
            frameSize: frame_size,
        };
        let mut effect_settings = IPLDirectEffectSettings { numChannels: 1 };

        unsafe {
            let status = iplDirectEffectCreate(context, &mut audio_settings, &mut effect_settings, &mut effect);
            if status != IPLerror_IPL_STATUS_SUCCESS {
                panic!("Failed to create IPLDirectEffect: Status {}", status);
            }
        }
        SteamDirectEffect { effect }
    }

    pub fn apply(
        &mut self,
        input_channel: &[f32],
        distance_attenuation: f32,
        air_absorption: [f32; 3],
        flags_bitmask: u32,
        occlusion: f32,
        transmission: [f32; 3],
        out_channel: &mut [f32],
    ) {
        let mut input_data = input_channel.as_ptr() as *mut f32;
        let mut input_buffer = IPLAudioBuffer {
            numChannels: 1,
            numSamples: input_channel.len() as i32,
            data: &mut input_data,
        };

        let mut out_data = out_channel.as_mut_ptr();
        let mut out_buffer = IPLAudioBuffer {
            numChannels: 1,
            numSamples: out_channel.len() as i32,
            data: &mut out_data,
        };

        let mut effect_flags = IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYDISTANCEATTENUATION
            | IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYAIRABSORPTION;

        let enable_occlusion = (flags_bitmask & (1 << 2)) != 0;
        let enable_transmission = (flags_bitmask & (1 << 3)) != 0;

        if enable_occlusion {
            effect_flags |= IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYOCCLUSION;
        }
        if enable_transmission {
            effect_flags |= IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYTRANSMISSION;
        }

        let mut params = IPLDirectEffectParams {
            flags: effect_flags,
            transmissionType: IPLTransmissionType_IPL_TRANSMISSIONTYPE_FREQDEPENDENT,
            distanceAttenuation: distance_attenuation,
            airAbsorption: air_absorption,
            directivity: 0.0,
            occlusion,
            transmission,
        };

        unsafe {
            iplDirectEffectApply(self.effect, &mut params, &mut input_buffer, &mut out_buffer);
        }
    }
}

impl Drop for SteamDirectEffect {
    fn drop(&mut self) {
        unsafe {
            iplDirectEffectRelease(&mut self.effect);
        }
    }
}

pub struct SteamBinauralEffect {
    effect: IPLBinauralEffect,
}

unsafe impl Send for SteamBinauralEffect {}
unsafe impl Sync for SteamBinauralEffect {}

impl SteamBinauralEffect {
    pub fn new(context: IPLContext, sample_rate: i32, frame_size: i32, hrtf: IPLHRTF) -> Self {
        let mut effect: IPLBinauralEffect = ptr::null_mut();
        let mut audio_settings = IPLAudioSettings {
            samplingRate: sample_rate,
            frameSize: frame_size,
        };
        let mut effect_settings = IPLBinauralEffectSettings { hrtf };

        unsafe {
            let status = iplBinauralEffectCreate(context, &mut audio_settings, &mut effect_settings, &mut effect);
            if status != IPLerror_IPL_STATUS_SUCCESS {
                panic!("Failed to create IPLBinauralEffect: Status {}", status);
            }
        }
        SteamBinauralEffect { effect }
    }

    pub fn apply(
        &mut self,
        input_channel: &[f32],
        direction: IPLVector3,
        hrtf: IPLHRTF,
        out_stereo_l: &mut [f32],
        out_stereo_r: &mut [f32],
    ) {
        let mut input_data = input_channel.as_ptr() as *mut f32;
        let mut input_buffer = IPLAudioBuffer {
            numChannels: 1,
            numSamples: input_channel.len() as i32,
            data: &mut input_data,
        };

        let mut out_channels = [out_stereo_l.as_mut_ptr(), out_stereo_r.as_mut_ptr()];
        let mut out_buffer = IPLAudioBuffer {
            numChannels: 2,
            numSamples: out_stereo_l.len() as i32,
            data: out_channels.as_mut_ptr(),
        };

        let mut params = IPLBinauralEffectParams {
            direction,
            interpolation: IPLHRTFInterpolation_IPL_HRTFINTERPOLATION_NEAREST,
            spatialBlend: 1.0,
            hrtf,
            peakDelays: std::ptr::null_mut(),
        };

        unsafe {
            iplBinauralEffectApply(self.effect, &mut params, &mut input_buffer, &mut out_buffer);
        }
    }
}

impl Drop for SteamBinauralEffect {
    fn drop(&mut self) {
        unsafe {
            iplBinauralEffectRelease(&mut self.effect);
        }
    }
}

pub fn get_relative_direction(
    context: IPLContext,
    source_pos: [f32; 3],
    listener_pos: [f32; 3],
    listener_fwd: [f32; 3],
    listener_up: [f32; 3],
) -> IPLVector3 {
    let source = IPLVector3 { x: source_pos[0], y: source_pos[1], z: source_pos[2] };
    let listener = IPLVector3 { x: listener_pos[0], y: listener_pos[1], z: listener_pos[2] };
    let ahead = IPLVector3 { x: listener_fwd[0], y: listener_fwd[1], z: listener_fwd[2] };
    let up = IPLVector3 { x: listener_up[0], y: listener_up[1], z: listener_up[2] };

    unsafe {
        iplCalculateRelativeDirection(context, source, listener, ahead, up)
    }
}

fn make_vector(d: usize, u: usize, v: usize, cd: f32, cu: f32, cv: f32, center: [i32; 3]) -> IPLVector3 {
    let mut coords = [0.0f32; 3];
    coords[d] = cd + center[d] as f32;
    coords[u] = cu + center[u] as f32;
    coords[v] = cv + center[v] as f32;
    IPLVector3 { x: coords[0], y: coords[1], z: coords[2] }
}

fn ray_triangle_intersect(
    orig: [f32; 3],
    dir: [f32; 3],
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
) -> Option<f32> {
    const EPSILON: f32 = 1e-6;
    let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
    let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

    let h = [
        dir[1] * edge2[2] - dir[2] * edge2[1],
        dir[2] * edge2[0] - dir[0] * edge2[2],
        dir[0] * edge2[1] - dir[1] * edge2[0],
    ];

    let a = edge1[0] * h[0] + edge1[1] * h[1] + edge1[2] * h[2];
    if a > -EPSILON && a < EPSILON {
        return None;
    }

    let f = 1.0 / a;
    let s = [orig[0] - v0[0], orig[1] - v0[1], orig[2] - v0[2]];
    let u = f * (s[0] * h[0] + s[1] * h[1] + s[2] * h[2]);
    if u < 0.0 || u > 1.0 {
        return None;
    }

    let q = [
        s[1] * edge1[2] - s[2] * edge1[1],
        s[2] * edge1[0] - s[0] * edge1[2],
        s[0] * edge1[1] - s[1] * edge1[0],
    ];

    let v = f * (dir[0] * q[0] + dir[1] * q[1] + dir[2] * q[2]);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * (edge2[0] * q[0] + edge2[1] * q[1] + edge2[2] * q[2]);
    if t > EPSILON {
        Some(t)
    } else {
        None
    }
}

pub fn calculate_occlusion_and_transmission(
    _scene: IPLScene,
    source_pos: [f32; 3],
    listener_pos: [f32; 3],
) -> (f32, [f32; 3]) {
    if IS_SCENE_COMMITTING.load(Ordering::Acquire) {
        return (1.0, [1.0, 1.0, 1.0]);
    }

    let safe_mesh_lock = match ACTIVE_SAFE_MESH.lock() {
        Ok(lock) => lock,
        Err(_) => return (1.0, [1.0, 1.0, 1.0]),
    };

    let safe_mesh = match safe_mesh_lock.as_ref() {
        Some(mesh) => mesh,
        None => return (1.0, [1.0, 1.0, 1.0]),
    };

    let direction_vec = [
        listener_pos[0] - source_pos[0],
        listener_pos[1] - source_pos[1],
        listener_pos[2] - source_pos[2],
    ];
    let distance = (direction_vec[0]*direction_vec[0] + direction_vec[1]*direction_vec[1] + direction_vec[2]*direction_vec[2]).sqrt();

    if distance < 1e-3 {
        return (1.0, [1.0, 1.0, 1.0]);
    }

    let dir_normalized = [
        direction_vec[0] / distance,
        direction_vec[1] / distance,
        direction_vec[2] / distance,
    ];

    let mut closest_t = distance;
    let mut hit_material_idx = -1;

    for (i, tri) in safe_mesh.triangles.iter().enumerate() {
        let v0_idx = tri.indices[0] as usize;
        let v1_idx = tri.indices[1] as usize;
        let v2_idx = tri.indices[2] as usize;

        if v0_idx >= safe_mesh.vertices.len() || v1_idx >= safe_mesh.vertices.len() || v2_idx >= safe_mesh.vertices.len() {
            continue;
        }

        let p0 = safe_mesh.vertices[v0_idx];
        let p1 = safe_mesh.vertices[v1_idx];
        let p2 = safe_mesh.vertices[v2_idx];

        let v0 = [p0.x, p0.y, p0.z];
        let v1 = [p1.x, p1.y, p1.z];
        let v2 = [p2.x, p2.y, p2.z];

        if let Some(t) = ray_triangle_intersect(source_pos, dir_normalized, v0, v1, v2) {
            if t > 1e-3 && t < closest_t {
                closest_t = t;
                if i < safe_mesh.materials.len() {
                    hit_material_idx = safe_mesh.materials[i];
                }
            }
        }
    }

    if hit_material_idx < 0 {
        return (1.0, [1.0, 1.0, 1.0]);
    }

    let mat_idx = hit_material_idx as usize;
    let transmission = if mat_idx < ACOUSTIC_MATERIALS.len() {
        ACOUSTIC_MATERIALS[mat_idx].transmission
    } else {
        [0.1, 0.1, 0.1]
    };

    (0.0, transmission)
}

pub fn rebuild_acoustic_mesh(
    scene: IPLScene,
    _context: IPLContext,
    voxels: &[u8; 262144],
    center: [i32; 3]
) {
    let mut vertices: Vec<IPLVector3> = Vec::new();
    let mut triangles: Vec<IPLTriangle> = Vec::new();
    let mut material_indices: Vec<i32> = Vec::new();

    let mut air_blocks = 0f32;
    let mut stone_blocks = 0f32;

    for d in 0..3 {
        let u = (d + 1) % 3;
        let v = (d + 2) % 3;

        for slice in 0..64 {
            for side in 0..2 {
                let mut mask = [[0u8; 64]; 64];

                for u_coord in 0..64 {
                    for v_coord in 0..64 {
                        let mut coords = [0; 3];
                        coords[d] = slice;
                        coords[u] = u_coord;
                        coords[v] = v_coord;

                        let idx = (coords[0] * 4096) + (coords[1] * 64) + coords[2];
                        let current_val = voxels[idx];

                        if current_val == 0 {
                            if d == 0 { air_blocks += 1.0 / 3.0; }
                        } else if current_val == 1 {
                            if d == 0 { stone_blocks += 1.0 / 3.0; }
                        }

                        if current_val > 0 {
                            let mut neighbor_coords = coords;
                            let has_neighbor = if side == 0 {
                                if slice > 0 {
                                    neighbor_coords[d] = slice - 1;
                                    let n_idx = (neighbor_coords[0] * 4096) + (neighbor_coords[1] * 64) + neighbor_coords[2];
                                    voxels[n_idx] > 0
                                } else {
                                    false
                                }
                            } else {
                                if slice < 63 {
                                    neighbor_coords[d] = slice + 1;
                                    let n_idx = (neighbor_coords[0] * 4096) + (neighbor_coords[1] * 64) + neighbor_coords[2];
                                    voxels[n_idx] > 0
                                } else {
                                    false
                                }
                            };

                            if !has_neighbor {
                                mask[u_coord][v_coord] = current_val;
                            }
                        }
                    }
                }

                let mut visited = [[false; 64]; 64];
                for u_s in 0..64 {
                    for v_s in 0..64 {
                        let m = mask[u_s][v_s];
                        if m > 0 && !visited[u_s][v_s] {
                            let mut w = 1;
                            while u_s + w < 64 && mask[u_s + w][v_s] == m && !visited[u_s + w][v_s] {
                                w += 1;
                            }

                            let mut h = 1;
                            'outer: while v_s + h < 64 {
                                for k in 0..w {
                                    if mask[u_s + k][v_s + h] != m || visited[u_s + k][v_s + h] {
                                        break 'outer;
                                    }
                                }
                                h += 1;
                            }

                            for ku in 0..w {
                                for kv in 0..h {
                                    visited[u_s + ku][v_s + kv] = true;
                                }
                            }

                            let cd = if side == 0 { slice as f32 } else { (slice + 1) as f32 };
                            let u_start = u_s as f32;
                            let u_end = (u_s + w) as f32;
                            let v_start = v_s as f32;
                            let v_end = (v_s + h) as f32;

                            let base_idx = vertices.len() as i32;

                            vertices.push(make_vector(d, u, v, cd, u_start, v_start, center));
                            vertices.push(make_vector(d, u, v, cd, u_end, v_start, center));
                            vertices.push(make_vector(d, u, v, cd, u_end, v_end, center));
                            vertices.push(make_vector(d, u, v, cd, u_start, v_end, center));

                            triangles.push(IPLTriangle { indices: [base_idx, base_idx + 3, base_idx + 2] });
                            triangles.push(IPLTriangle { indices: [base_idx, base_idx + 2, base_idx + 1] });

                            let mat_idx = (m as i32 - 1).clamp(0, 3);
                            material_indices.push(mat_idx);
                            material_indices.push(mat_idx);
                        }
                    }
                }
            }
        }
    }

    if vertices.is_empty() {
        return;
    }

    unsafe {
        IS_SCENE_COMMITTING.store(true, Ordering::Release);

        let mut active_mesh_lock = ACTIVE_STATIC_MESH.lock().unwrap();

        if let Some(old_mesh) = active_mesh_lock.take() {
            iplStaticMeshRemove(old_mesh.0, scene);
            let mut raw_ptr = old_mesh.0;
            iplStaticMeshRelease(&mut raw_ptr);
        }

        let mut static_mesh: IPLStaticMesh = ptr::null_mut();
        let mut materials_array = ACOUSTIC_MATERIALS.to_vec();

        let mut settings = IPLStaticMeshSettings {
            numVertices: vertices.len() as i32,
            numTriangles: triangles.len() as i32,
            vertices: vertices.as_mut_ptr(),
            triangles: triangles.as_mut_ptr(),
            materialIndices: material_indices.as_mut_ptr(),
            materials: materials_array.as_mut_ptr(),
            numMaterials: materials_array.len() as i32,
        };

        let status = iplStaticMeshCreate(scene, &mut settings, &mut static_mesh);
        if status == IPLerror_IPL_STATUS_SUCCESS {
            iplStaticMeshAdd(static_mesh, scene);
            iplSceneCommit(scene);

            *active_mesh_lock = Some(SendStaticMesh(static_mesh));

            let mut safe_mesh_lock = ACTIVE_SAFE_MESH.lock().unwrap();
            *safe_mesh_lock = Some(SafeMesh {
                vertices,
                triangles,
                materials: material_indices,
            });

            let mesh_ref = (*safe_mesh_lock).as_ref().unwrap();

            let mut total_area = 0.0f32;
            let mut weighted_absorption = 0.0f32;

            for (i, tri) in mesh_ref.triangles.iter().enumerate() {
                let p0 = mesh_ref.vertices[tri.indices[0] as usize];
                let p1 = mesh_ref.vertices[tri.indices[1] as usize];
                let p2 = mesh_ref.vertices[tri.indices[2] as usize];

                let e1 = [p1.x - p0.x, p1.y - p0.y, p1.z - p0.z];
                let e2 = [p2.x - p0.x, p2.y - p0.y, p2.z - p0.z];
                let cross = [
                    e1[1] * e2[2] - e1[2] * e2[1],
                    e1[2] * e2[0] - e1[0] * e2[2],
                    e1[0] * e2[1] - e1[1] * e2[0],
                ];
                let tri_area = 0.5 * (cross[0]*cross[0] + cross[1]*cross[1] + cross[2]*cross[2]).sqrt();
                total_area += tri_area;

                if i < mesh_ref.materials.len() {
                    let mat_idx = mesh_ref.materials[i] as usize;
                    if mat_idx < ACOUSTIC_MATERIALS.len() {
                        let avg_abs = (ACOUSTIC_MATERIALS[mat_idx].absorption[0]
                            + ACOUSTIC_MATERIALS[mat_idx].absorption[1]
                            + ACOUSTIC_MATERIALS[mat_idx].absorption[2]) / 3.0f32;
                        weighted_absorption += tri_area * avg_abs;
                    }
                }
            }

            let avg_abs = if total_area > 0.1 { weighted_absorption / total_area } else { 0.15f32 };
            let room_volume = air_blocks;

            let leakage = 10.0f32;
            let base_t60 = (0.161f32 * room_volume) / (total_area * avg_abs + leakage);

            let low_decay = base_t60 * 1.5;
            let mid_decay = base_t60;
            let high_decay = base_t60 * 0.5;

            let stone_ratio = stone_blocks / 262144.0;
            let is_underground = stone_ratio > 0.4;

            if is_underground && room_volume > 1000.0 {
                let t60 = [
                    low_decay.clamp(0.2, 5.0),
                    mid_decay.clamp(0.2, 4.0),
                    high_decay.clamp(0.1, 2.0),
                ];
                let wet_mix = (stone_ratio * 2.0).clamp(0.0, 0.4);
                ACTIVE_ENVIRONMENT.store_relaxed(t60, wet_mix);
            } else {
                ACTIVE_ENVIRONMENT.store_relaxed([0.1, 0.1, 0.1], 0.0);
            }

            let loaded_env = ACTIVE_ENVIRONMENT.load_relaxed();
            println!(
                "[Rust Daemon] Meshing committed. Triangles: {}, Est. Volume: {} m3, T60: [{:.2}s, {:.2}s, {:.2}s], Wet Send: {:.1}%",
                mesh_ref.triangles.len(),
                room_volume as u32,
                loaded_env.0[0],
                loaded_env.0[1],
                loaded_env.0[2],
                loaded_env.1 * 100.0
            );
        } else {
            eprintln!("[Rust Daemon] Failed to compile static mesh: Status {}", status);
        }

        IS_SCENE_COMMITTING.store(false, Ordering::Release);
    }
}