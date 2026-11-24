// Copyright 2012-2021 Emilie Gillet. MIT licence (the notice is in mod.rs).
// Rust port: Copyright (C) 2026 Pro Wave contributors, GPL-3.0-or-later.
//
// Ported from stmlib (utils/random.h, dsp/dsp.h, dsp/units.h, dsp/filter.h,
// dsp/parameter_interpolator.h, dsp/hysteresis_quantizer.h, dsp/hysteresis_filter.h,
// dsp/cosine_oscillator.h, utils/gate_flags.h) and Beads (dsp/frame.h, dsp/limiter.h,
// dsp/sine_oscillator.h, dsp/mu_law.h). Each function does the C++ arithmetic in the same
// order, so the same input gives the same floats.

use super::luts;
use std::ops::{Add, AddAssign, Mul, MulAssign, Sub};

/// A stereo sample (`beads::FloatFrame`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    pub l: f32,
    pub r: f32,
}

impl Frame {
    pub const ZERO: Frame = Frame { l: 0.0, r: 0.0 };
    pub const fn new(l: f32, r: f32) -> Self {
        Self { l, r }
    }
}

impl Add for Frame {
    type Output = Frame;
    fn add(self, o: Frame) -> Frame {
        Frame::new(self.l + o.l, self.r + o.r)
    }
}

impl Sub for Frame {
    type Output = Frame;
    fn sub(self, o: Frame) -> Frame {
        Frame::new(self.l - o.l, self.r - o.r)
    }
}

impl Mul<f32> for Frame {
    type Output = Frame;
    fn mul(self, k: f32) -> Frame {
        Frame::new(self.l * k, self.r * k)
    }
}

impl AddAssign for Frame {
    fn add_assign(&mut self, o: Frame) {
        self.l += o.l;
        self.r += o.r;
    }
}

impl MulAssign<f32> for Frame {
    fn mul_assign(&mut self, k: f32) {
        self.l *= k;
        self.r *= k;
    }
}

/// `stmlib::Random`: the 32-bit linear congruential generator. Beads keeps one for the
/// whole module; here each effect has its own, seeded at a fixed value, so a bounce and a
/// second run of the same input give the same grains.
#[derive(Clone, Copy, Debug)]
pub struct Random(pub u32);

impl Random {
    /// stmlib's starting state.
    pub const DEFAULT_SEED: u32 = 0x21;

    pub fn word(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }

    pub fn float(&mut self) -> f32 {
        self.word() as f32 / 4_294_967_296.0
    }

    /// `CenteredUniform`.
    pub fn centered(&mut self) -> f32 {
        2.0 * self.float() - 1.0
    }

    /// `PseudoGaussian`.
    pub fn pseudo_gaussian(&mut self) -> f32 {
        let u = self.float();
        u * (u * (4.0 * u - 6.0) + 4.0) - 1.0
    }

    /// `BoundedRandomFloat`.
    pub fn bounded(&mut self, spread: f32) -> f32 {
        let u = self.float();
        let v = self.float();
        0.5 + (u - 0.5) * (spread * v).min(1.0)
    }
}

/// `MAKE_INTEGRAL_FRACTIONAL`.
#[inline]
pub fn integral_fractional(x: f32) -> (i32, f32) {
    let i = x as i32;
    (i, x - i as f32)
}

/// `stmlib::Interpolate` (linear, `index` in 0..1 times `size`).
#[inline]
pub fn interpolate(table: &[f32], index: f32, size: f32) -> f32 {
    let (i, f) = integral_fractional(index * size);
    let last = table.len().saturating_sub(2);
    let i = (i.max(0) as usize).min(last);
    let a = table[i];
    let b = table[i + 1];
    a + (b - a) * f
}

#[inline]
pub fn smooth_step(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

#[inline]
pub fn crossfade(a: f32, b: f32, fade: f32) -> f32 {
    a + (b - a) * fade
}

#[inline]
pub fn soft_limit(x: f32) -> f32 {
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

#[inline]
pub fn soft_clip(x: f32) -> f32 {
    if x < -3.0 {
        -1.0
    } else if x > 3.0 {
        1.0
    } else {
        soft_limit(x)
    }
}

#[inline]
pub fn clip16(x: i32) -> i32 {
    x.clamp(-32768, 32767)
}

/// `ONE_POLE`.
#[inline]
pub fn one_pole(out: &mut f32, input: f32, coefficient: f32) {
    *out += coefficient * (input - *out);
}

/// `SLOPE`.
#[inline]
pub fn slope(out: &mut f32, input: f32, positive: f32, negative: f32) {
    let error = input - *out;
    *out += (if error > 0.0 { positive } else { negative }) * error;
}

/// `CONSTRAIN`.
#[inline]
pub fn constrain(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// `stmlib::SemitonesToRatio` (table lookup, valid from -128 to +127 semitones).
#[inline]
pub fn semitones_to_ratio(semitones: f32) -> f32 {
    let (i, f) = integral_fractional(semitones + 128.0);
    let hi = i.clamp(0, 255) as usize;
    let lo = ((f * 256.0) as i32).clamp(0, 255) as usize;
    luts::PITCH_RATIO_HIGH[hi] * luts::PITCH_RATIO_LOW[lo]
}

/// `beads::Lin2MuLaw`.
pub fn lin_to_mu_law(pcm: i16) -> u8 {
    let mut pcm = i32::from(pcm) >> 2;
    let mask: i32 = if pcm < 0 {
        pcm = -pcm;
        0x7f
    } else {
        0xff
    };
    if pcm > 8159 {
        pcm = 8159;
    }
    pcm += 0x84 >> 2;
    let seg = if pcm <= 0x3f {
        0
    } else if pcm <= 0x7f {
        1
    } else if pcm <= 0xff {
        2
    } else if pcm <= 0x1ff {
        3
    } else if pcm <= 0x3ff {
        4
    } else if pcm <= 0x7ff {
        5
    } else if pcm <= 0xfff {
        6
    } else if pcm <= 0x1fff {
        7
    } else {
        8
    };
    if seg >= 8 {
        (0x7f ^ mask) as u8
    } else {
        let uval = (seg << 4) | ((pcm >> (seg + 1)) & 0x0f);
        ((uval ^ mask) & 0xff) as u8
    }
}

#[inline]
pub fn mu_law_to_lin(u: u8) -> f32 {
    luts::ULAW[usize::from(u)]
}

/// `stmlib::ParameterInterpolator`: steps from the old value to the new one over a block.
/// The value reached is written back with [`Interp::store`], as the C++ destructor does (the
/// value after the steps actually taken, not the target).
#[derive(Clone, Copy, Debug)]
pub struct Interp {
    value: f32,
    increment: f32,
}

impl Interp {
    #[inline]
    pub fn new(state: f32, new_value: f32, size: usize) -> Self {
        Self {
            value: state,
            increment: (new_value - state) / size as f32,
        }
    }
    #[inline]
    pub fn step(&mut self) -> f32 {
        self.value += self.increment;
        self.value
    }
    #[inline]
    pub fn store(&self, state: &mut f32) {
        *state = self.value;
    }
}

/// `stmlib::FrequencyApproximation::FREQUENCY_FAST`'s tangent.
#[inline]
pub fn tan_fast(f: f32) -> f32 {
    use std::f64::consts::PI;
    let a = (f64::from(3.260e-01f32) * PI * PI * PI) as f32;
    let b = (f64::from(1.823e-01f32) * PI * PI * PI * PI * PI) as f32;
    let f2 = f * f;
    f * (PI as f32 + f2 * (a + b * f2))
}

/// `stmlib::Svf`, the state-variable filter.
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    g: f32,
    r: f32,
    h: f32,
    state_1: f32,
    state_2: f32,
}

impl Svf {
    pub fn init(&mut self) {
        // set_f_q<FREQUENCY_DIRTY>(0.01f, 100.0f)
        use std::f64::consts::PI;
        let a = (f64::from(3.736e-01f32) * PI * PI * PI) as f32;
        let f = 0.01f32;
        self.g = f * (PI as f32 + a * f * f);
        self.r = 1.0 / 100.0;
        self.h = 1.0 / (1.0 + self.r * self.g + self.g * self.g);
        self.state_1 = 0.0;
        self.state_2 = 0.0;
    }
    pub fn set_f_q_fast(&mut self, f: f32, resonance: f32) {
        self.g = tan_fast(f);
        self.r = 1.0 / resonance;
        self.h = 1.0 / (1.0 + self.r * self.g + self.g * self.g);
    }
    pub fn set_from(&mut self, o: &Svf) {
        self.g = o.g;
        self.r = o.r;
        self.h = o.h;
    }
    #[inline]
    fn step(&mut self, input: f32) -> (f32, f32) {
        let hp = (input - self.r * self.state_1 - self.g * self.state_1 - self.state_2) * self.h;
        let bp = self.g * hp + self.state_1;
        self.state_1 = self.g * hp + bp;
        let lp = self.g * bp + self.state_2;
        self.state_2 = self.g * bp + lp;
        (lp, hp)
    }
    #[inline]
    pub fn low_pass(&mut self, input: f32) -> f32 {
        self.step(input).0
    }
    #[inline]
    pub fn high_pass(&mut self, input: f32) -> f32 {
        self.step(input).1
    }
}

/// `stmlib::HysteresisQuantizer2`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HysteresisQuantizer {
    num_steps: i32,
    hysteresis: f32,
    scale: f32,
    offset: f32,
    quantized: i32,
}

impl HysteresisQuantizer {
    pub fn init(&mut self, num_steps: i32, hysteresis: f32, symmetric: bool) {
        self.num_steps = num_steps;
        self.hysteresis = hysteresis;
        self.scale = (if symmetric { num_steps - 1 } else { num_steps }) as f32;
        self.offset = if symmetric { 0.0 } else { -0.5 };
        self.quantized = 0;
    }
    pub fn num_steps(&self) -> i32 {
        self.num_steps
    }
    pub fn process(&mut self, value: f32) -> i32 {
        let mut value = value * self.scale;
        value += self.offset;
        let sign = if value > self.quantized as f32 {
            -1.0
        } else {
            1.0
        };
        let q = (value + sign * self.hysteresis + 0.5) as i32;
        let q = q.clamp(0, (self.num_steps - 1).max(0));
        self.quantized = q;
        q
    }
}

/// `stmlib::HysteresisFilter`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HysteresisFilter {
    value: f32,
    threshold: f32,
}

impl HysteresisFilter {
    pub fn init(&mut self, threshold: f32) {
        self.value = 0.0;
        self.threshold = threshold;
    }
    pub fn process(&mut self, value: f32) -> f32 {
        if self.threshold == 0.0 {
            self.value = value;
        } else {
            let error = value - self.value;
            if error > self.threshold {
                self.value = value - self.threshold;
            } else if error < -self.threshold {
                self.value = value + self.threshold;
            }
        }
        self.value
    }
}

/// `stmlib::CosineOscillator` in its approximate mode.
#[derive(Clone, Copy, Debug, Default)]
pub struct CosineOscillator {
    y1: f32,
    y0: f32,
    iir_coefficient: f32,
    initial_amplitude: f32,
}

impl CosineOscillator {
    pub fn init_approximate(&mut self, frequency: f32) {
        let mut sign = 16.0f32;
        let mut frequency = frequency - 0.25;
        if frequency < 0.0 {
            frequency = -frequency;
        } else if frequency > 0.5 {
            frequency -= 0.5;
        } else {
            sign = -16.0;
        }
        self.iir_coefficient = sign * frequency * (1.0 - 2.0 * frequency);
        self.initial_amplitude = self.iir_coefficient * 0.25;
        self.y1 = self.initial_amplitude;
        self.y0 = 0.5;
    }
    pub fn value(&self) -> f32 {
        self.y1 + 0.5
    }
    pub fn advance(&mut self) -> f32 {
        let temp = self.y0;
        self.y0 = self.iir_coefficient * self.y0 - self.y1;
        self.y1 = temp;
        temp + 0.5
    }
}

/// `beads::SineOscillator`: a sine between 0 and 1 for slow internal modulations.
#[derive(Clone, Copy, Debug, Default)]
pub struct SineOscillator {
    phase: f32,
    frequency: f32,
}

impl SineOscillator {
    pub fn init(&mut self, frequency: f32) {
        self.phase = 0.0;
        self.frequency = frequency;
    }
    pub fn phase(&self) -> f32 {
        self.phase
    }
    pub fn render(&mut self) -> f32 {
        self.phase += self.frequency;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        0.5 + 0.5 * interpolate(&luts::SINE, self.phase, 1024.0)
    }
    pub fn render_at(&mut self, frequency: f32) -> f32 {
        self.frequency = frequency;
        self.render()
    }
}

/// `beads::Limiter`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Limiter {
    peak: f32,
    peak_lp: f32,
    decay: f32,
    max_peak: f32,
}

impl Limiter {
    pub fn init(&mut self, decay: f32, max_peak: f32) {
        self.peak = 0.5;
        self.peak_lp = 0.5;
        self.decay = decay;
        self.max_peak = max_peak;
    }
    #[inline]
    pub fn process_one(&mut self, f: &mut Frame) {
        let peak = f.l.abs().max(f.r.abs()).min(40.0);
        slope(&mut self.peak, peak, 0.1, self.decay);
        let p = self.peak;
        slope(&mut self.peak_lp, p, 0.5, self.decay);
        let gain = if self.peak_lp <= self.max_peak {
            1.0
        } else {
            self.max_peak / self.peak_lp
        };
        f.l *= gain;
        f.r *= gain;
    }
    pub fn process(&mut self, frames: &mut [Frame]) {
        for f in frames {
            self.process_one(f);
        }
    }
}

/// `stmlib::GateFlags` bits.
pub const GATE_HIGH: u8 = 1;
pub const GATE_RISING: u8 = 2;
pub const GATE_FALLING: u8 = 4;

/// `stmlib::ExtractGateFlags`.
#[inline]
pub fn extract_gate_flags(previous: u8, current: bool) -> u8 {
    let previous = previous & GATE_HIGH;
    if current {
        if previous != 0 {
            GATE_HIGH
        } else {
            GATE_RISING | GATE_HIGH
        }
    } else if previous != 0 {
        GATE_FALLING
    } else {
        0
    }
}

/// The PITCH pot law (`PotChannel::LAW_DETUNE`): `detune` -1..1 to semitones, resting on
/// 0, 1, 3, 4, 7, 12 and 24.
pub fn interpolate_detune(detune: f32) -> f32 {
    const INTERVALS: [f32; 8] = [0.0, 1.0, 3.0, 4.0, 7.0, 12.0, 24.0, 24.0];
    let sign = if detune < 0.0 { -1.0 } else { 1.0 };
    let detune = detune * sign * 6.0;
    let (i, f) = integral_fractional(detune);
    let f = ((f - 0.333_333_33) * 1.5).max(0.0);
    let i = i.clamp(0, 6) as usize;
    crossfade(INTERVALS[i], INTERVALS[i + 1], smooth_step(f)) * sign
}
