// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 SoundCraft contributors (https://github.com/storytold/soundcraft)
//
// Auto Pan: moves a signal between the left and right channels with an LFO.
// Constant-power law: as the pan position moves right the left channel's gain falls
// and the right's rises; at center both gains are 1.

use std::f32::consts::{FRAC_PI_4, PI, SQRT_2, TAU};

/// The LFO shape at `t` in 0..=1 of a cycle, from -1 to 1.
pub fn wave_at(waveform: usize, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let tri = if t < 0.5 {
        4.0 * t - 1.0
    } else {
        3.0 - 4.0 * t
    };
    match waveform {
        1 => tri,
        2 => {
            if t < 0.5 {
                -1.0
            } else {
                1.0
            }
        }
        3 => 2.0 * t - 1.0,
        4 => 1.0 - 2.0 * t,
        5 => 2.0 * (PI * t).sin() - 1.0,
        _ => -(TAU * t).cos(),
    }
}

/// Auto-Pan DSP node. Zero allocations during processing.
#[derive(Debug, Clone)]
pub struct AutoPanNode {
    sr: f32,
    phase: f64,
}

impl AutoPanNode {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sr: sample_rate.clamp(8_000.0, 384_000.0),
            phase: 0.0,
        }
    }

    /// Process a stereo frame:
    /// - `rate_hz`: LFO frequency in Hz (0.01..50.0 Hz)
    /// - `width`: 0..1 (depth of pan modulation)
    /// - `waveform`: 0=Sine, 1=Triangle, 2=Square, 3=Saw Up, 4=Saw Down
    /// - `out_db`: Output trim in dB (-24..+12 dB)
    #[inline]
    pub fn process_frame(
        &mut self,
        l: f32,
        r: f32,
        rate_hz: f32,
        width: f32,
        waveform: usize,
        out_db: f32,
    ) -> (f32, f32) {
        let hz = (rate_hz as f64).clamp(0.001, 100.0);
        let step = hz / (self.sr as f64);
        self.phase = (self.phase + step).fract();
        if self.phase < 0.0 {
            self.phase += 1.0;
        }

        let lfo = wave_at(waveform, self.phase as f32);
        let pos = (lfo * width.clamp(0.0, 1.0)).clamp(-1.0, 1.0);

        let out_gain = 10.0f32.powf(out_db.clamp(-24.0, 12.0) / 20.0);

        // Constant power pan: sin² + cos² = 1, both gains 1 at center (pos = 0)
        let theta = (pos + 1.0) * FRAC_PI_4;
        let l_gain = theta.cos().max(0.0) * SQRT_2 * out_gain;
        let r_gain = theta.sin().max(0.0) * SQRT_2 * out_gain;

        (l * l_gain, r * r_gain)
    }
}
