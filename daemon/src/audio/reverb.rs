const DELAY_SIZES: [usize; 4] = [977, 1277, 1637, 1997];

pub struct FdnReverb {
    buffers: [Vec<f32>; 4],
    indices: [usize; 4],
    lowpass_states: [f32; 4],
}

impl FdnReverb {
    pub fn new() -> Self {
        FdnReverb {
            buffers: [
                vec![0.0; DELAY_SIZES[0]],
                vec![0.0; DELAY_SIZES[1]],
                vec![0.0; DELAY_SIZES[2]],
                vec![0.0; DELAY_SIZES[3]],
            ],
            indices: [0; 4],
            lowpass_states: [0.0; 4],
        }
    }

    pub fn process(&mut self, input: f32, t60: [f32; 3], wet_mix: f32) -> f32 {
        if wet_mix <= 0.005 {
            return 0.0;
        }

        let mid_decay = t60[1].max(0.1);
        let feedback_gain: [f32; 4] = [
            ( -60.0 * (DELAY_SIZES[0] as f32 / 48000.0) / mid_decay ).exp2().clamp(0.0, 0.95),
            ( -60.0 * (DELAY_SIZES[1] as f32 / 48000.0) / mid_decay ).exp2().clamp(0.0, 0.95),
            ( -60.0 * (DELAY_SIZES[2] as f32 / 48000.0) / mid_decay ).exp2().clamp(0.0, 0.95),
            ( -60.0 * (DELAY_SIZES[3] as f32 / 48000.0) / mid_decay ).exp2().clamp(0.0, 0.95),
        ];

        let mut d = [0.0f32; 4];
        for i in 0..4 {
            d[i] = self.buffers[i][self.indices[i]];
        }

        let tmp0 = d[0] + d[1];
        let tmp1 = d[2] + d[3];
        let tmp2 = d[0] - d[1];
        let tmp3 = d[2] - d[3];

        let out = [
            0.5 * (tmp0 + tmp1),
            0.5 * (tmp0 - tmp1),
            0.5 * (tmp2 + tmp3),
            0.5 * (tmp2 - tmp3),
        ];

        let high_decay_ratio = (t60[2] / t60[1]).clamp(0.1, 0.9);
        let lpf_coefficient = 1.0 - high_decay_ratio;

        for i in 0..4 {
            let output_with_feedback = input + out[i] * feedback_gain[i];
            self.lowpass_states[i] = self.lowpass_states[i] * lpf_coefficient + output_with_feedback * (1.0 - lpf_coefficient);
            self.buffers[i][self.indices[i]] = self.lowpass_states[i];
            self.indices[i] = (self.indices[i] + 1) % DELAY_SIZES[i];
        }

        (out[0] + out[1] + out[2] + out[3]) * 0.25 * wet_mix
    }
}