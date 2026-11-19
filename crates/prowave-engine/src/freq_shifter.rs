// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 SoundCraft contributors (https://github.com/storytold/soundcraft)
//
// Bode Frequency Shifter: moves every frequency in the signal by the same number of hertz.
// Unlike a pitch shifter, which multiplies frequencies, a frequency shifter adds a constant to
// them, so harmonic sounds become inharmonic. The signal goes through two chains of allpass
// filters whose outputs differ in phase by 90° (the in-phase and quadrature parts of the
// analytic signal). Multiplying them by a cosine and a sine at the shift frequency and adding or
// subtracting the products keeps only the upper or only the lower sideband (single-sideband
// modulation). The filters run in `f64`.

use std::f64::consts::{PI, TAU};

/// Number of allpass sections in each branch.
pub const SECTIONS: usize = 10;
const QUADRATURE_LOW_HZ: f64 = 20.0;
pub const MAX_FEEDBACK: f32 = 70.0;

/// Arithmetic-geometric mean of `a` and `b`.
fn agm(mut a: f64, mut b: f64) -> f64 {
    for _ in 0..64 {
        let (m, g) = (0.5 * (a + b), (a * b).sqrt());
        a = m;
        b = g;
        if (a - b).abs() <= 1.0e-15 * a {
            break;
        }
    }
    a
}

/// Allpass coefficients for a 90° phase-difference network at `sr`.
fn design_quadrature(sr: f64) -> Option<[f64; 2 * SECTIONS]> {
    let tb = 2.0 * QUADRATURE_LOW_HZ / sr;
    if !tb.is_finite() || tb <= 0.0 || tb >= 0.25 {
        return None;
    }
    let k = ((1.0 - 2.0 * tb) * PI / 4.0).tan().powi(2);
    let kc = ((1.0 - k) * (1.0 + k)).sqrt();
    let q = (-PI * agm(1.0, kc) / agm(1.0, k)).exp();
    let order = (4 * SECTIONS + 1) as f64;
    let mut coefs = [0.0f64; 2 * SECTIONS];
    for (idx, coef) in coefs.iter_mut().enumerate() {
        let i = (idx + 1) as f64;
        let mut num = 0.0;
        for m in 0..64i32 {
            let sign = if m % 2 == 0 { 1.0 } else { -1.0 };
            let term = sign * q.powi(m * (m + 1)) * ((2 * m + 1) as f64 * i * PI / order).sin();
            num += term;
            if term.abs() < 1.0e-30 {
                break;
            }
        }
        let mut den = 0.5;
        for m in 1..64i32 {
            let sign = if m % 2 == 0 { 1.0 } else { -1.0 };
            let term = sign * q.powi(m * m) * (2.0 * f64::from(m) * i * PI / order).cos();
            den += term;
            if term.abs() < 1.0e-30 {
                break;
            }
        }
        let w = q.powf(0.25) * num / den;
        let w2 = w * w;
        let x = ((1.0 - w2 * k) * (1.0 - w2 / k)).max(0.0).sqrt() / (1.0 + w2);
        *coef = (1.0 - x) / (1.0 + x);
    }
    coefs.iter().all(|a| a.is_finite() && (0.0..1.0).contains(a)).then_some(coefs)
}

fn quadrature_coefs(sr: f64) -> [f64; 2 * SECTIONS] {
    design_quadrature(sr)
        .or_else(|| design_quadrature(48_000.0))
        .unwrap_or([0.0; 2 * SECTIONS])
}

#[inline]
fn flush64(x: f64) -> f64 {
    if x.abs() > 1.0e-30 && x.is_finite() {
        x
    } else {
        0.0
    }
}

/// One `(a - z⁻²) / (1 - a z⁻²)` allpass section.
#[derive(Debug, Clone, Copy, Default)]
pub struct Section {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Section {
    #[inline]
    pub fn tick(&mut self, a: f64, x: f64) -> f64 {
        let y = flush64(a * (x + self.y2) - self.x2);
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[inline]
fn chain(sections: &mut [Section; SECTIONS], coefs: &[f64; SECTIONS], x: f64) -> f64 {
    sections.iter_mut().zip(coefs.iter()).fold(x, |v, (s, &a)| s.tick(a, v))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelState {
    pub in_phase: [Section; SECTIONS],
    pub quadrature: [Section; SECTIONS],
    pub delayed: f64,
    pub last: f64,
}

/// Bode Frequency Shifter DSP node. Zero allocations during processing.
#[derive(Debug, Clone)]
pub struct FreqShifterNode {
    sr: f64,
    in_phase_coefs: [f64; SECTIONS],
    quadrature_coefs: [f64; SECTIONS],
    channels: [ChannelState; 2],
    phase: f64,
}

impl FreqShifterNode {
    pub fn new(sample_rate: f32) -> Self {
        let sr = (sample_rate as f64).clamp(8_000.0, 384_000.0);
        let mut node = Self {
            sr,
            in_phase_coefs: [0.0; SECTIONS],
            quadrature_coefs: [0.0; SECTIONS],
            channels: [ChannelState::default(); 2],
            phase: 0.0,
        };
        let coefs = quadrature_coefs(sr);
        for (i, a) in coefs.iter().enumerate() {
            let side = if i % 2 == 0 {
                &mut node.in_phase_coefs
            } else {
                &mut node.quadrature_coefs
            };
            if let Some(slot) = side.get_mut(i / 2) {
                *slot = *a;
            }
        }
        node
    }

    /// Process a stereo frame: `freq_hz` is frequency shift (+/-), `mode` (0=Up, 1=Down, 2=Both, 3=Stereo),
    /// `feedback` is 0..0.70, `mix` is 0..1. Zero allocations.
    #[inline]
    pub fn process_frame(
        &mut self,
        l: f32,
        r: f32,
        freq_hz: f32,
        mode: usize,
        feedback: f32,
        mix: f32,
    ) -> (f32, f32) {
        let (sin, cos) = (TAU * self.phase).sin_cos();
        let step = f64::from(freq_hz) / self.sr;
        self.phase = (self.phase + step).fract();
        if self.phase < 0.0 {
            self.phase += 1.0;
        }

        let fb = f64::from(feedback.clamp(0.0, MAX_FEEDBACK / 100.0));
        let mix = mix.clamp(0.0, 1.0);

        let (side_l, side_r) = match mode {
            1 => (-1.0f64, -1.0f64), // Down
            2 => (0.0f64, 0.0f64),   // Both
            3 => (-1.0f64, 1.0f64),  // Stereo (L down, R up)
            _ => (1.0f64, 1.0f64),   // Up
        };

        // Left channel
        let dry_l = if l.is_finite() { l } else { 0.0 };
        let input_l = f64::from(dry_l) + fb * self.channels[0].last;
        let i_l = chain(&mut self.channels[0].in_phase, &self.in_phase_coefs, input_l);
        let q_l = chain(
            &mut self.channels[0].quadrature,
            &self.quadrature_coefs,
            self.channels[0].delayed,
        );
        self.channels[0].delayed = input_l;
        let wet_l = i_l * cos - side_l * q_l * sin;
        self.channels[0].last = flush64(wet_l);
        let out_l = dry_l * (1.0 - mix) + self.channels[0].last as f32 * mix;

        // Right channel
        let dry_r = if r.is_finite() { r } else { 0.0 };
        let input_r = f64::from(dry_r) + fb * self.channels[1].last;
        let i_r = chain(&mut self.channels[1].in_phase, &self.in_phase_coefs, input_r);
        let q_r = chain(
            &mut self.channels[1].quadrature,
            &self.quadrature_coefs,
            self.channels[1].delayed,
        );
        self.channels[1].delayed = input_r;
        let wet_r = i_r * cos - side_r * q_r * sin;
        self.channels[1].last = flush64(wet_r);
        let out_r = dry_r * (1.0 - mix) + self.channels[1].last as f32 * mix;

        (out_l, out_r)
    }
}
