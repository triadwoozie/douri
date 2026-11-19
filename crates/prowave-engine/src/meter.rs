// Copyright (c) 2026 ArtCraft Team and the SoundCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Level meters: DAW-style peak meter, RMS meter, and an ITU-R BS.1770 loudness meter
//! (momentary, short-term and gated integrated LUFS plus 4× oversampled true peak).
//!
//! None of the `process` methods allocate heap memory once initialized.

pub const MIN_DB: f32 = -144.0;

/// Converts linear amplitude to decibels full-scale (clamped at [`MIN_DB`]).
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 1e-7 {
        MIN_DB
    } else {
        (20.0 * gain.log10()).max(MIN_DB)
    }
}

/// Converts decibels full-scale to linear amplitude.
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    if db <= MIN_DB {
        0.0
    } else {
        10.0f32.powf(db / 20.0)
    }
}

/// Flushes denormals and non-finite values in feedback paths to zero.
#[inline]
pub fn flush(x: f32) -> f32 {
    let a = x.abs();
    if (a > 1.0e-25) & (a < f32::INFINITY) {
        x
    } else {
        0.0
    }
}

/// Sanitizes a sample rate to 1 kHz ..= 1.536 MHz.
#[inline]
pub fn sane_sr(sr: f32) -> f32 {
    if sr.is_finite() && sr >= 1000.0 {
        sr.min(1_536_000.0)
    } else {
        48_000.0
    }
}

/// Normalized biquad coefficients (`a0 = 1`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Default for Coeffs {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Coeffs {
    pub const IDENTITY: Coeffs = Coeffs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };
}

/// A biquad section with its own state (transposed direct form II).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Biquad {
    pub coeffs: Coeffs,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn new(coeffs: Coeffs) -> Self {
        Self {
            coeffs,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let c = &self.coeffs;
        let y = c.b0 * x + self.z1;
        self.z1 = flush(c.b1 * x - c.a1 * y + self.z2);
        self.z2 = flush(c.b2 * x - c.a2 * y);
        if y.is_finite() {
            y
        } else {
            self.reset();
            0.0
        }
    }
}

/// Peak meter with instant attack, linear-in-dB release, peak hold and clip latch.
#[derive(Debug, Clone)]
pub struct PeakMeter {
    sr: f32,
    level_db: f32,
    hold_db: f32,
    hold_left: f32,
    hold_secs: f32,
    release_db_per_sec: f32,
    clipped: bool,
}

impl PeakMeter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sr: sane_sr(sample_rate),
            level_db: MIN_DB,
            hold_db: MIN_DB,
            hold_left: 0.0,
            hold_secs: 2.0,
            release_db_per_sec: 20.0,
            clipped: false,
        }
    }

    pub fn set_release_db_per_sec(&mut self, rate: f32) {
        if rate.is_finite() {
            self.release_db_per_sec = rate.clamp(0.1, 1000.0);
        }
    }

    pub fn set_hold_secs(&mut self, secs: f32) {
        if secs.is_finite() {
            self.hold_secs = secs.clamp(0.0, 60.0);
        }
    }

    pub fn feed(&mut self, peak: f32, frames: usize) {
        let dt = frames as f32 / self.sr;
        if peak >= 1.0 {
            self.clipped = true;
        }
        let peak_db = gain_to_db(peak);
        let decayed = (self.level_db - self.release_db_per_sec * dt).max(MIN_DB);
        self.level_db = decayed.max(peak_db);
        if peak_db >= self.hold_db {
            self.hold_db = peak_db;
            self.hold_left = self.hold_secs;
        } else if self.hold_left > 0.0 {
            self.hold_left -= dt;
        } else {
            self.hold_db = (self.hold_db - self.release_db_per_sec * dt).max(self.level_db);
        }
    }

    pub fn level_db(&self) -> f32 {
        self.level_db
    }

    pub fn peak_hold_db(&self) -> f32 {
        self.hold_db
    }

    pub fn clipped(&self) -> bool {
        self.clipped
    }

    pub fn reset_clip(&mut self) {
        self.clipped = false;
    }

    pub fn reset(&mut self) {
        self.level_db = MIN_DB;
        self.hold_db = MIN_DB;
        self.hold_left = 0.0;
        self.clipped = false;
    }
}

/// Exponentially averaged RMS meter.
#[derive(Debug, Clone)]
pub struct RmsMeter {
    sr: f32,
    coef: f32,
    ms: f64,
}

impl RmsMeter {
    pub fn new(sample_rate: f32) -> Self {
        let mut m = Self {
            sr: sane_sr(sample_rate),
            coef: 0.0,
            ms: 0.0,
        };
        m.set_window_ms(300.0);
        m
    }

    pub fn set_window_ms(&mut self, ms: f32) {
        let s = (if ms.is_finite() { ms.clamp(1.0, 10_000.0) } else { 300.0 }) * 0.001 * self.sr;
        self.coef = if s < 1.0e-3 { 0.0 } else { (-1.0 / s).exp() };
    }

    pub fn process(&mut self, samples: &[f32]) {
        let k = f64::from(1.0 - self.coef);
        for &x in samples {
            let x = if x.is_finite() { f64::from(x) } else { 0.0 };
            self.ms += (x * x - self.ms) * k;
        }
        if self.ms < 1e-30 {
            self.ms = 0.0;
        }
    }

    pub fn rms_db(&self) -> f32 {
        gain_to_db(self.ms.sqrt() as f32)
    }

    pub fn reset(&mut self) {
        self.ms = 0.0;
    }
}

const SUB_BLOCKS: usize = 30; // 3 s of 100 ms sub-blocks
const HIST_MIN: f32 = -70.0;
const HIST_STEP: f32 = 0.05;
const HIST_BINS: usize = 1800; // -70 .. +20 LUFS
const TP_PHASES: usize = 4;
const TP_TAPS: usize = 12;

/// ITU-R BS.1770 / EBU R128 loudness meter.
#[derive(Debug, Clone)]
pub struct LoudnessMeter {
    sr: f32,
    weights: Vec<f32>,
    filters: Vec<[Biquad; 2]>,
    block_len: usize,
    block_pos: usize,
    block_acc: Vec<f64>,
    sub: [f64; SUB_BLOCKS],
    sub_idx: usize,
    sub_filled: usize,
    hist_count: Vec<u64>,
    hist_energy: Vec<f64>,
    tp_coefs: [[f32; TP_PHASES]; TP_TAPS],
    tp_hist: Vec<[f32; 2 * TP_TAPS]>,
    tp_pos: Vec<usize>,
    true_peak: f32,
    max_momentary: f32,
}

/// Loudness from mean-square energy (BS.1770 eq. 2).
#[inline]
fn lufs(energy: f64) -> f32 {
    if energy <= 1e-20 {
        MIN_DB
    } else {
        (-0.691 + 10.0 * energy.log10()) as f32
    }
}

/// K-weighting pre-filter (high shelf) and RLB high-pass designed for any sample rate.
fn k_weighting(sr: f32) -> [Coeffs; 2] {
    let sr = f64::from(sr);
    let prewarp = |f0: f64| (std::f64::consts::PI * f0.min(sr * 0.49) / sr).tan();
    // Stage 1: shelving.
    let (f0, g, q) = (1_681.974_450_955_533, 3.999_843_853_973_347, 0.707_175_236_955_419_6);
    let k = prewarp(f0);
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let s1 = Coeffs {
        b0: ((vh + vb * k / q + k * k) / a0) as f32,
        b1: (2.0 * (k * k - vh) / a0) as f32,
        b2: ((vh - vb * k / q + k * k) / a0) as f32,
        a1: (2.0 * (k * k - 1.0) / a0) as f32,
        a2: ((1.0 - k / q + k * k) / a0) as f32,
    };
    // Stage 2: high-pass.
    let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
    let k = prewarp(f0);
    let a0 = 1.0 + k / q + k * k;
    let s2 = Coeffs {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: (2.0 * (k * k - 1.0) / a0) as f32,
        a2: ((1.0 - k / q + k * k) / a0) as f32,
    };
    [s1, s2]
}

impl LoudnessMeter {
    /// Creates a meter for `channels` channels at `sample_rate`.
    /// Pre-allocates all buffers so `process` methods never allocate.
    pub fn new(sample_rate: f32, channels: usize) -> Self {
        let sr = sane_sr(sample_rate);
        let channels = channels.clamp(1, 64);
        let weights = if channels == 6 {
            vec![1.0, 1.0, 1.0, 0.0, 1.41, 1.41]
        } else {
            vec![1.0; channels]
        };
        let [c1, c2] = k_weighting(sr);
        // Interpolation filter: windowed sinc at the original Nyquist, split into 4 phases.
        let mut tp_coefs = [[0.0f32; TP_PHASES]; TP_TAPS];
        let total = TP_TAPS * TP_PHASES;
        let center = (total as f64 - 1.0) / 2.0;
        for i in 0..total {
            let t = (i as f64 - center) / TP_PHASES as f64;
            let sinc = if t.abs() < 1e-12 {
                1.0
            } else {
                (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t)
            };
            let x = i as f64 / (total as f64 - 1.0);
            let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * x).cos()
                + 0.08 * (4.0 * std::f64::consts::PI * x).cos();
            if let Some(slot) = tp_coefs
                .get_mut(i / TP_PHASES)
                .and_then(|tap| tap.get_mut(i % TP_PHASES))
            {
                *slot = (sinc * w) as f32;
            }
        }
        for p in 0..TP_PHASES {
            let s: f32 = tp_coefs.iter().filter_map(|tap| tap.get(p)).sum();
            if s.abs() > 1e-6 {
                tp_coefs
                    .iter_mut()
                    .filter_map(|tap| tap.get_mut(p))
                    .for_each(|c| *c /= s);
            }
        }
        Self {
            sr,
            weights,
            filters: vec![[Biquad::new(c1), Biquad::new(c2)]; channels],
            block_len: ((sr * 0.1).round() as usize).max(1),
            block_pos: 0,
            block_acc: vec![0.0; channels],
            sub: [0.0; SUB_BLOCKS],
            sub_idx: 0,
            sub_filled: 0,
            hist_count: vec![0; HIST_BINS],
            hist_energy: vec![0.0; HIST_BINS],
            tp_coefs,
            tp_hist: vec![[0.0; 2 * TP_TAPS]; channels],
            tp_pos: vec![0; channels],
            true_peak: 0.0,
            max_momentary: MIN_DB,
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    pub fn reset(&mut self) {
        for f in &mut self.filters {
            f[0].reset();
            f[1].reset();
        }
        self.block_pos = 0;
        self.block_acc.iter_mut().for_each(|v| *v = 0.0);
        self.sub = [0.0; SUB_BLOCKS];
        self.sub_idx = 0;
        self.sub_filled = 0;
        self.hist_count.iter_mut().for_each(|v| *v = 0);
        self.hist_energy.iter_mut().for_each(|v| *v = 0.0);
        self.tp_hist.iter_mut().for_each(|h| *h = [0.0; 2 * TP_TAPS]);
        self.tp_pos.iter_mut().for_each(|p| *p = 0);
        self.true_peak = 0.0;
        self.max_momentary = MIN_DB;
    }

    /// Feeds one stereo frame (left, right). Never allocates.
    #[inline]
    pub fn process_sample(&mut self, l: f32, r: f32) {
        let ch = self.filters.len().min(2);
        let samples = [if l.is_finite() { l } else { 0.0 }, if r.is_finite() { r } else { 0.0 }];
        for c in 0..ch {
            let x = samples[c];
            if let Some([f1, f2]) = self.filters.get_mut(c) {
                let y = f2.process_sample(f1.process_sample(x));
                if let Some(acc) = self.block_acc.get_mut(c) {
                    *acc += f64::from(y) * f64::from(y);
                }
            }
            self.true_peak_sample(c, x);
        }
        self.block_pos += 1;
        if self.block_pos >= self.block_len {
            self.finish_sub_block();
        }
    }

    /// Feeds interleaved stereo audio (`[L0, R0, L1, R1, ...]`). Never allocates.
    pub fn process_interleaved(&mut self, samples: &[f32]) {
        let frames = samples.len() / 2;
        for i in 0..frames {
            let l = samples[i * 2];
            let r = samples[i * 2 + 1];
            self.process_sample(l, r);
        }
    }

    /// Feeds planar channel slices. Never allocates.
    pub fn process_planar(&mut self, channels: &[&[f32]]) {
        let ch = self.filters.len().min(channels.len());
        let frames = channels.iter().take(ch).map(|c| c.len()).fold(usize::MAX, usize::min);
        if frames == usize::MAX || frames == 0 {
            return;
        }
        for n in 0..frames {
            for c in 0..ch {
                let x = channels.get(c).and_then(|b| b.get(n)).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
                if let Some([f1, f2]) = self.filters.get_mut(c) {
                    let y = f2.process_sample(f1.process_sample(x));
                    if let Some(acc) = self.block_acc.get_mut(c) {
                        *acc += f64::from(y) * f64::from(y);
                    }
                }
                self.true_peak_sample(c, x);
            }
            self.block_pos += 1;
            if self.block_pos >= self.block_len {
                self.finish_sub_block();
            }
        }
    }

    fn true_peak_sample(&mut self, c: usize, x: f32) {
        let (Some(h), Some(pos)) = (self.tp_hist.get_mut(c), self.tp_pos.get_mut(c)) else {
            return;
        };
        *pos = (*pos + TP_TAPS - 1) % TP_TAPS;
        let p = *pos;
        for slot in [p, p + TP_TAPS] {
            if let Some(v) = h.get_mut(slot) {
                *v = x;
            }
        }
        let Some(w) = h.get(p..p + TP_TAPS).and_then(|w| <&[f32; TP_TAPS]>::try_from(w).ok()) else {
            return;
        };
        let mut y = [0.0f32; TP_PHASES];
        for (cs, s) in self.tp_coefs.iter().zip(w) {
            for (acc, k) in y.iter_mut().zip(cs) {
                *acc += k * s;
            }
        }
        let mut peak = x.abs();
        for v in y {
            peak = peak.max(v.abs());
        }
        if peak > self.true_peak {
            self.true_peak = peak;
        }
    }

    fn finish_sub_block(&mut self) {
        let len = self.block_pos.max(1) as f64;
        let mut energy = 0.0;
        for (acc, w) in self.block_acc.iter_mut().zip(&self.weights) {
            energy += f64::from(*w) * *acc / len;
            *acc = 0.0;
        }
        self.block_pos = 0;
        if let Some(s) = self.sub.get_mut(self.sub_idx) {
            *s = energy;
        }
        self.sub_idx = (self.sub_idx + 1) % SUB_BLOCKS;
        self.sub_filled = (self.sub_filled + 1).min(SUB_BLOCKS);
        if self.sub_filled >= 4 {
            let z = self.recent_energy(4);
            let l = lufs(z);
            self.max_momentary = self.max_momentary.max(l);
            if l > HIST_MIN {
                let bin = (((l - HIST_MIN) / HIST_STEP) as usize).min(HIST_BINS - 1);
                if let (Some(c), Some(e)) = (self.hist_count.get_mut(bin), self.hist_energy.get_mut(bin)) {
                    *c += 1;
                    *e += z;
                }
            }
        }
    }

    /// Mean energy of the last `n` completed 100 ms sub-blocks.
    fn recent_energy(&self, n: usize) -> f64 {
        let n = n.min(self.sub_filled);
        if n == 0 {
            return 0.0;
        }
        let mut sum = 0.0;
        for k in 1..=n {
            let idx = (self.sub_idx + SUB_BLOCKS - k) % SUB_BLOCKS;
            sum += self.sub.get(idx).copied().unwrap_or(0.0);
        }
        sum / n as f64
    }

    /// Momentary loudness (400 ms window), LUFS.
    pub fn momentary_lufs(&self) -> f32 {
        if self.sub_filled < 4 {
            MIN_DB
        } else {
            lufs(self.recent_energy(4))
        }
    }

    /// Short-term loudness (3 s window, or what is available so far), LUFS.
    pub fn short_term_lufs(&self) -> f32 {
        if self.sub_filled < 4 {
            MIN_DB
        } else {
            lufs(self.recent_energy(SUB_BLOCKS))
        }
    }

    /// Highest momentary loudness seen.
    pub fn max_momentary_lufs(&self) -> f32 {
        self.max_momentary
    }

    /// Gated integrated loudness (absolute gate -70 LUFS, relative gate -10 LU), LUFS.
    pub fn integrated_lufs(&self) -> f32 {
        let (mut n, mut e) = (0u64, 0.0f64);
        for (c, s) in self.hist_count.iter().zip(&self.hist_energy) {
            n += c;
            e += s;
        }
        if n == 0 {
            return MIN_DB;
        }
        let gate = lufs(e / n as f64) - 10.0;
        let (mut n2, mut e2) = (0u64, 0.0f64);
        for (i, (c, s)) in self.hist_count.iter().zip(&self.hist_energy).enumerate() {
            let center = HIST_MIN + (i as f32 + 0.5) * HIST_STEP;
            if center > gate {
                n2 += c;
                e2 += s;
            }
        }
        if n2 == 0 {
            MIN_DB
        } else {
            lufs(e2 / n2 as f64)
        }
    }

    /// Maximum true peak (4× oversampled) in dBTP.
    pub fn true_peak_db(&self) -> f32 {
        gain_to_db(self.true_peak)
    }

    /// Maximum true peak (linear).
    pub fn true_peak_linear(&self) -> f32 {
        self.true_peak
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudness_meter_silence_returns_min_db() {
        let mut m = LoudnessMeter::new(48_000.0, 2);
        assert_eq!(m.momentary_lufs(), MIN_DB);
        assert_eq!(m.integrated_lufs(), MIN_DB);
        assert_eq!(m.true_peak_db(), MIN_DB);

        let silence = vec![0.0f32; 48_000];
        m.process_interleaved(&silence);
        assert_eq!(m.momentary_lufs(), MIN_DB);
        assert_eq!(m.integrated_lufs(), MIN_DB);
    }

    #[test]
    fn loudness_meter_sine_wave_measures_expected_level() {
        let sr = 48_000.0;
        let mut m = LoudnessMeter::new(sr, 2);
        // 1 kHz sine at 0 dBFS amplitude (0.707 RMS)
        let frames = 48_000 * 2; // 2 seconds
        let mut interleaved = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr).sin();
            interleaved.push(s);
            interleaved.push(s);
        }
        m.process_interleaved(&interleaved);
        let l = m.integrated_lufs();
        // A full-scale 1 kHz stereo sine wave (peak 1.0 in both L and R) in BS.1770 measures 0.0 LUFS
        assert!((l - 0.0).abs() < 0.2, "LUFS was {l}");
        let tp = m.true_peak_db();
        assert!((tp - 0.0).abs() < 0.2, "True peak was {tp}");
    }
}
