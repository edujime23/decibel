use crate::steam_audio::*;
use std::ptr;

pub struct SteamDirectEffect {
    effect: IPLDirectEffect,
}

// Explicitly mark raw pointer wrappers as thread-safe since they are strictly
// owned and mutated by a single active sound pipeline inside the CPAL thread.
unsafe impl Send for SteamDirectEffect {}
unsafe impl Sync for SteamDirectEffect {}

impl SteamDirectEffect {
    pub fn new(context: IPLContext, sample_rate: i32, frame_size: i32) -> Self {
        let mut effect: IPLDirectEffect = ptr::null_mut();
        let mut audio_settings = IPLAudioSettings {
            samplingRate: sample_rate,
            frameSize: frame_size,
        };
        let mut effect_settings = IPLDirectEffectSettings {
            numChannels: 1,
        };

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

        let mut params = IPLDirectEffectParams {
            flags: IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYDISTANCEATTENUATION
                | IPLDirectEffectFlags_IPL_DIRECTEFFECTFLAGS_APPLYAIRABSORPTION,
            transmissionType: IPLTransmissionType_IPL_TRANSMISSIONTYPE_FREQDEPENDENT,
            distanceAttenuation: distance_attenuation,
            airAbsorption: air_absorption,
            directivity: 0.0,
            occlusion: 0.0,
            transmission: [1.0, 1.0, 1.0], // Correct field mappings for Steam Audio v4
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
        let mut effect_settings = IPLBinauralEffectSettings {
            hrtf,
        };

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
            peakDelays: std::ptr::null_mut(), // Added missing peakDelays pointer initialisation
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
    source_pos: [f32; 3],
    listener_pos: [f32; 3],
    listener_fwd: [f32; 3],
    listener_up: [f32; 3],
) -> IPLVector3 {
    let t = [
        source_pos[0] - listener_pos[0],
        source_pos[1] - listener_pos[1],
        source_pos[2] - listener_pos[2],
    ];

    let norm_fwd = normalize(listener_fwd);
    let norm_up = normalize(listener_up);

    let r = cross(norm_fwd, norm_up);
    let norm_r = normalize(r);

    let x_local = dot(t, norm_r);
    let y_local = dot(t, norm_up);
    let z_local = -dot(t, norm_fwd);

    IPLVector3 { x: x_local, y: y_local, z: z_local }
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt();
    if len < 1e-6 {
        v
    } else {
        [v[0]/len, v[1]/len, v[2]/len]
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1]*b[2] - a[2]*b[1],
        a[2]*b[0] - a[0]*b[2],
        a[0]*b[1] - a[1]*b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0]*b[0] + a[1]*b[1] + a[2]*b[2]
}