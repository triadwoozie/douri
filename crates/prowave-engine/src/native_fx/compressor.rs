// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from transit-audio/transit (MIT OR Apache-2.0, Copyright (c) 2024 transit contributors).

//! Native Compressor: Studio-grade dynamic range compressor with soft knee, stereo linking,
//! logarithmic gain reduction computation and parallel dry/wet mixing.
//!
//! Logic Pro equivalent: Compressor (Audio FX > Dynamics > Compressor).

use super::{block_effect, stored_values, NativeFx, NativeParam};

pub const ID: &str = "prowave:compressor";
pub const NAME: &str = "Compressor";

pub const THRESHOLD: usize = 0;
pub const RATIO: usize = 1;
pub const ATTACK: usize = 2;
pub const RELEASE: usize = 3;
pub const KNEE: usize = 4;
pub const MAKEUP: usize = 5;
pub const MIX: usize = 6;
pub const PARAMS_COUNT: usize = 7;

pub static PARAMS: [NativeParam; PARAMS_COUNT] = [
    NativeParam {
        name: "Threshold",
        min: -60.0,
        max: 0.0,
        default: -20.0,
        unit: "dB",
        choices: &[],
        integer: true,
        live: true,
    },
    NativeParam {
        name: "Ratio",
        min: 1.0,
        max: 100.0,
        default: 4.0,
        unit: ":1",
        choices: &[],
        integer: true,
        live: true,
    },
    NativeParam {
        name: "Attack",
        min: 1.0,
        max: 300.0,
        default: 10.0,
        unit: "ms",
        choices: &[],
        integer: true,
        live: true,
    },
    NativeParam {
        name: "Release",
        min: 5.0,
        max: 3000.0,
        default: 100.0,
        unit: "ms",
        choices: &[],
        integer: true,
        live: true,
    },
    NativeParam {
        name: "Knee",
        min: 0.0,
        max: 24.0,
        default: 6.0,
        unit: "dB",
        choices: &[],
        integer: true,
        live: true,
    },
    NativeParam {
        name: "Makeup",
        min: 0.0,
        max: 40.0,
        default: 0.0,
        unit: "dB",
        choices: &[],
        integer: true,
        live: true,
    },
    NativeParam {
        name: "Mix",
        min: 0.0,
        max: 100.0,
        default: 100.0,
        unit: "%",
        choices: &[],
        integer: true,
        live: true,
    },
];

pub fn value_text(index: usize, value: i32) -> String {
    match index {
        THRESHOLD => format!("{value:+}dB"),
        RATIO => format!("{value}:1"),
        ATTACK => format!("{value}ms"),
        RELEASE => format!("{value}ms"),
        KNEE => format!("{value}dB"),
        MAKEUP => format!("{value:+}dB"),
        MIX => format!("{value}%"),
        _ => String::new(),
    }
}

/// Static soft-knee gain computer: returns gain reduction in dB (>= 0).
#[inline]
pub fn compress_gr(level_db: f32, threshold: f32, ratio: f32, knee: f32) -> f32 {
    let slope = 1.0 - 1.0 / ratio.max(1.0);
    let over = level_db - threshold;
    if knee > 1e-3 && 2.0 * over.abs() <= knee {
        let t = over + knee * 0.5;
        slope * t * t / (2.0 * knee)
    } else if over > 0.0 {
        slope * over
    } else {
        0.0
    }
}

#[inline]
fn ballistic(state: f32, target: f32, att: f32, rel: f32) -> f32 {
    let c = if target > state { att } else { rel };
    target + (state - target) * c
}

pub struct Compressor {
    rate: u32,
    applied: [i32; PARAMS_COUNT],
    envelope_gr_db: f32,
    att_coef: f32,
    rel_coef: f32,
    makeup_gain: f32,
    mix_dry: f32,
    mix_wet: f32,
}

impl Compressor {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let mut comp = Self {
            rate: rate.max(8_000),
            applied: stored_values(&PARAMS, values),
            envelope_gr_db: 0.0,
            att_coef: 0.0,
            rel_coef: 0.0,
            makeup_gain: 1.0,
            mix_dry: 0.0,
            mix_wet: 1.0,
        };
        comp.recalculate();
        comp
    }

    fn recalculate(&mut self) {
        let sr = self.rate as f32;
        let att_sec = (self.applied[ATTACK].max(1) as f32 * 0.001).max(1e-5);
        let rel_sec = (self.applied[RELEASE].max(5) as f32 * 0.001).max(1e-4);
        self.att_coef = (-1.0 / (att_sec * sr)).exp();
        self.rel_coef = (-1.0 / (rel_sec * sr)).exp();
        self.makeup_gain = 10f32.powf(self.applied[MAKEUP] as f32 / 20.0);
        let mix = (self.applied[MIX] as f32 / 100.0).clamp(0.0, 1.0);
        self.mix_wet = mix;
        self.mix_dry = 1.0 - mix;
    }

    #[inline]
    pub fn process_frame(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let peak = in_l.abs().max(in_r.abs()).max(1e-12);
        let level_db = 20.0 * peak.log10();

        let target_gr_db = compress_gr(
            level_db,
            self.applied[THRESHOLD] as f32,
            self.applied[RATIO] as f32,
            self.applied[KNEE] as f32,
        );

        self.envelope_gr_db = ballistic(
            self.envelope_gr_db,
            target_gr_db,
            self.att_coef,
            self.rel_coef,
        );

        let gr_lin = 10f32.powf(-self.envelope_gr_db / 20.0) * self.makeup_gain;

        let wet_l = in_l * gr_lin;
        let wet_r = in_r * gr_lin;

        let out_l = in_l * self.mix_dry + wet_l * self.mix_wet;
        let out_r = in_r * self.mix_dry + wet_r * self.mix_wet;

        (out_l, out_r)
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        for i in 0..n {
            let (ol, or) = self.process_frame(left[i], right[i]);
            left[i] = ol;
            right[i] = or;
        }
    }

    fn latency_samples(&self) -> usize {
        0
    }
}

block_effect!(Compressor);

impl NativeFx for Compressor {
    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = PARAMS.get(index) else { return };
        let mut v = self.applied;
        v[index] = p.stored(value);
        if v != self.applied {
            self.applied = v;
            self.recalculate();
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn gain_reduction_db(&self) -> f32 {
        self.envelope_gr_db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_reduces_loud_input() {
        let mut comp = Compressor::new(48000, &[-12.0, 4.0, 1.0, 50.0, 6.0, 0.0, 100.0]);

        for i in 0..2000 {
            let s = (i as f32 * 0.1).sin();
            let (ol, _) = comp.process_frame(s, s);
            assert!(ol.is_finite());
        }

        assert!(comp.gain_reduction_db() > 4.0);
    }
}
