// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! A streaming sample-rate converter between the project's rate and the rate the texture
//! engine runs at (48, 32 or 24 kHz, as on the module). Windowed-sinc (Kaiser) interpolation
//! with exact integer stepping, so a run is the same every time. Memory is allocated in
//! [`Resampler::new`]; `push` never allocates.

use super::dsp::Frame;

/// Zero crossings of the sinc on each side.
const ZEROS: usize = 16;
/// Table points per zero crossing.
const OVERSAMPLE: usize = 256;
/// Kaiser window beta (about 80 dB of stop-band attenuation).
const BETA: f64 = 8.0;
/// The pass band ends at this fraction of the lower rate's Nyquist frequency.
const PASS: f64 = 0.92;

/// The modified Bessel function I0 (series), for the Kaiser window.
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..64 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// The kernel, sampled in units of zero crossings from 0 to `ZEROS` (plus a guard point).
fn kernel_table() -> Vec<f32> {
    let n = ZEROS * OVERSAMPLE + 2;
    let norm = bessel_i0(BETA);
    (0..n)
        .map(|i| {
            let u = i as f64 / OVERSAMPLE as f64;
            if u >= ZEROS as f64 {
                return 0.0;
            }
            let sinc = if i == 0 {
                1.0
            } else {
                let x = std::f64::consts::PI * u;
                x.sin() / x
            };
            let r = u / ZEROS as f64;
            let w = bessel_i0(BETA * (1.0 - r * r).max(0.0).sqrt()) / norm;
            (sinc * w) as f32
        })
        .collect()
}

/// Converts a stream at `from` Hz to `to` Hz. Each output sample is emitted once the input
/// it needs has arrived; [`Resampler::lag_seconds`] is the most that takes.
pub struct Resampler {
    from: u32,
    to: u32,
    bypass: bool,
    /// Half the kernel's length, in input samples.
    half: i64,
    /// Kernel time scale: zero crossings per input sample.
    scale: f64,
    /// Output gain (the kernel's height at the cutoff).
    gain: f32,
    table: Vec<f32>,
    hist_l: Vec<f32>,
    hist_r: Vec<f32>,
    mask: usize,
    /// Input samples pushed.
    count: i64,
    /// The next output's position in input samples: `pos + num / to`.
    pos: i64,
    num: u64,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        let from = from.max(1);
        let to = to.max(1);
        let bypass = from == to;
        // Cutoff in cycles per input sample.
        let fc = 0.5 * PASS * (f64::from(to) / f64::from(from)).min(1.0);
        let scale = 2.0 * fc;
        let half = (ZEROS as f64 / scale).ceil() as i64;
        let len = ((2 * half + 4) as usize).next_power_of_two();
        Self {
            from,
            to,
            bypass,
            half,
            scale,
            gain: scale as f32,
            table: if bypass { Vec::new() } else { kernel_table() },
            hist_l: vec![0.0; if bypass { 0 } else { len }],
            hist_r: vec![0.0; if bypass { 0 } else { len }],
            mask: len - 1,
            count: 0,
            pos: 0,
            num: 0,
        }
    }

    /// Forgets the input so far (no allocation).
    pub fn reset(&mut self) {
        self.hist_l.fill(0.0);
        self.hist_r.fill(0.0);
        self.count = 0;
        self.pos = 0;
        self.num = 0;
    }

    /// The longest wait between an input and the output it completes, in seconds.
    pub fn lag_seconds(&self) -> f64 {
        if self.bypass {
            0.0
        } else {
            (self.half + 1) as f64 / f64::from(self.from)
        }
    }

    #[inline]
    fn weight(&self, distance: f64) -> f32 {
        let u = (distance.abs() * self.scale * OVERSAMPLE as f64) as f32;
        let i = u as usize;
        if i + 1 >= self.table.len() {
            return 0.0;
        }
        let f = u - i as f32;
        let a = self.table[i];
        let b = self.table[i + 1];
        (a + (b - a) * f) * self.gain
    }

    /// Takes one input frame and hands every output frame it completes to `emit`.
    #[inline]
    pub fn push(&mut self, x: Frame, mut emit: impl FnMut(Frame)) {
        if self.bypass {
            emit(x);
            return;
        }
        let at = (self.count as usize) & self.mask;
        self.hist_l[at] = x.l;
        self.hist_r[at] = x.r;
        self.count += 1;
        while self.pos + self.half < self.count {
            let t_frac = self.num as f64 / f64::from(self.to);
            let mut acc = Frame::ZERO;
            for n in (self.pos - self.half + 1)..=(self.pos + self.half) {
                if n < 0 {
                    continue;
                }
                let w = self.weight(t_frac + (self.pos - n) as f64);
                let k = (n as usize) & self.mask;
                acc.l += self.hist_l[k] * w;
                acc.r += self.hist_r[k] * w;
            }
            emit(acc);
            self.num += u64::from(self.from);
            while self.num >= u64::from(self.to) {
                self.num -= u64::from(self.to);
                self.pos += 1;
            }
        }
    }
}
