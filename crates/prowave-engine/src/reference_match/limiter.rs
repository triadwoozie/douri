//! Matchering's brickwall limiter, "Hyrax" (`matchering/limiter/hyrax.py`), and the SciPy
//! pieces it is made of: `maximum_filter1d`, `lfilter`, `filtfilt` and a first-order
//! `butter`. It works on the whole song at once (look-ahead and a zero-phase attack), so it is
//! offline only.
#![allow(clippy::needless_range_loop)]

use std::f64::consts::PI;

/// Matchering's `LimiterConfig` (its defaults). The hold and release filters are first-order
/// Butterworth low-passes, as Matchering's default orders are.
#[derive(Clone, Debug, PartialEq)]
pub struct LimiterConfig {
    /// Milliseconds.
    pub attack: f64,
    /// Milliseconds.
    pub hold: f64,
    /// Milliseconds.
    pub release: f64,
    pub attack_filter_coefficient: f64,
    /// Hz.
    pub hold_filter_coefficient: f64,
    /// Divided by `release` for the release filter's corner in Hz.
    pub release_filter_coefficient: f64,
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self {
            attack: 1.0,
            hold: 1.0,
            release: 3000.0,
            attack_filter_coefficient: -2.0,
            hold_filter_coefficient: 7.0,
            release_filter_coefficient: 800.0,
        }
    }
}

/// `matchering.utils.ms_to_samples`.
fn ms_to_samples(ms: f64, rate: u32) -> usize {
    (f64::from(rate) * ms * 1e-3) as usize
}

/// `scipy.ndimage.maximum_filter1d(x, size)` with its default `reflect` edges and origin 0:
/// the largest value in `[i - size/2, i - size/2 + size - 1]`. Reflected samples repeat ones
/// the window already holds, so the window is just cut at the ends.
fn max_filter(x: &[f64], before: usize, after: usize) -> Vec<f64> {
    let n = x.len();
    let mut out = vec![0.0; n];
    // A monotonic queue of indices with falling values.
    let mut q: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut next = 0usize;
    for i in 0..n {
        let hi = (i + after).min(n - 1);
        while next <= hi {
            while q.back().is_some_and(|&b| x[b] <= x[next]) {
                q.pop_back();
            }
            q.push_back(next);
            next += 1;
        }
        let lo = i.saturating_sub(before);
        while q.front().is_some_and(|&f| f < lo) {
            q.pop_front();
        }
        out[i] = q.front().map_or(0.0, |&f| x[f]);
    }
    out
}

/// A first-order IIR `b0 + b1 z^-1 / 1 + a1 z^-1` (SciPy's `lfilter`, transposed direct form II)
/// from state `z`.
fn lfilter1(b0: f64, b1: f64, a1: f64, x: &[f64], mut z: f64) -> Vec<f64> {
    let mut y = Vec::with_capacity(x.len());
    for &v in x {
        let o = b0 * v + z;
        z = b1 * v - a1 * o;
        y.push(o);
    }
    y
}

/// `scipy.signal.filtfilt(b, a, x)` for a first-order filter: odd extension of 3 × 2 samples at
/// each end, the forward pass started in its steady state for the first sample, the backward
/// pass for the last.
fn filtfilt1(b0: f64, b1: f64, a1: f64, x: &[f64]) -> Vec<f64> {
    let n = x.len();
    let edge = 6usize;
    if n <= edge {
        return x.to_vec();
    }
    let mut ext = Vec::with_capacity(n + 2 * edge);
    for k in (1..=edge).rev() {
        ext.push(2.0 * x[0] - x[k]);
    }
    ext.extend_from_slice(x);
    for k in 1..=edge {
        ext.push(2.0 * x[n - 1] - x[n - 1 - k]);
    }
    // lfilter_zi: solve (1 + a1) zi = b1 - a1 b0.
    let zi = (b1 - a1 * b0) / (1.0 + a1);
    let fwd = lfilter1(b0, b1, a1, &ext, zi * ext[0]);
    let rev: Vec<f64> = fwd.iter().rev().copied().collect();
    let back = lfilter1(b0, b1, a1, &rev, zi * rev[0]);
    let mut out: Vec<f64> = back.into_iter().rev().collect();
    out.truncate(n + edge);
    out.drain(..edge);
    out
}

/// `scipy.signal.butter(1, hz, fs=rate)` (a low-pass): `(b0, b1, a1)` worked out as SciPy does
/// (prewarped analogue prototype, bilinear transform).
fn butter1(hz: f64, rate: u32) -> (f64, f64, f64) {
    let wn = 2.0 * hz / f64::from(rate);
    let fs = 2.0;
    let warped = 2.0 * fs * (PI * wn / fs).tan();
    let fs2 = 2.0 * fs;
    let p = -warped;
    let pz = (fs2 + p) / (fs2 - p);
    let kz = warped * (1.0 / (fs2 - p));
    (kz, kz, -pz)
}

/// The gain Hyrax puts on each sample, given each sample's level `level` (its peak over the
/// channels) and the ceiling `threshold`. `None` when nothing goes over (Matchering returns the
/// audio untouched then).
pub fn gain(level: &[f64], threshold: f64, rate: u32, cfg: &LimiterConfig) -> Option<Vec<f64>> {
    let n = level.len();
    if n == 0 {
        return None;
    }
    // rectify
    let rect: Vec<f64> = level
        .iter()
        .map(|&v| if v <= threshold { 1.0 } else { v / threshold })
        .collect();
    if rect.iter().all(|&r| (r - 1.0).abs() <= 1e-8 + 1e-5) {
        return None;
    }
    // flip(1 / rectified)
    let hard: Vec<f64> = rect.iter().map(|r| 1.0 - 1.0 / r).collect();
    // attack: a centred maximum over 2 × odd(attack) - 1 samples, then filtfilt.
    let attack = ms_to_samples(cfg.attack, rate);
    let odd = if attack & 1 == 0 { attack + 1 } else { attack };
    let size = 2 * odd - 1;
    let slided = max_filter(&hard, size / 2, size - size / 2 - 1);
    let coef = (cfg.attack_filter_coefficient / attack.max(1) as f64).exp();
    let gain_attack = filtfilt1(1.0 - coef, 0.0, -coef, &slided);
    // hold / release: a trailing maximum over `hold` samples of the attack's maximum.
    let hold = ms_to_samples(cfg.hold, rate).max(1);
    let half = (hold - 1) / 2;
    // maximum_filter1d(pad(x, (half, 0)), hold)[:-half]: original samples [i - (hold/2 + half), i - (hold/2 + half) + hold - 1].
    let back = hold / 2 + half;
    let ahead = (hold - hold / 2 - 1).saturating_sub(half);
    let hold_in = max_filter(&slided, back, ahead);
    let (hb0, hb1, ha1) = butter1(cfg.hold_filter_coefficient, rate);
    let hold_out = lfilter1(hb0, hb1, ha1, &hold_in, 0.0);
    let (rb0, rb1, ra1) = butter1(cfg.release_filter_coefficient / cfg.release, rate);
    let rel_in: Vec<f64> = hold_in
        .iter()
        .zip(&hold_out)
        .map(|(a, b)| a.max(*b))
        .collect();
    let rel_out = lfilter1(rb0, rb1, ra1, &rel_in, 0.0);
    let mut g = Vec::with_capacity(n);
    for i in 0..n {
        let release = hold_out[i].max(rel_out[i]);
        g.push(1.0 - hard[i].max(gain_attack[i]).max(release));
    }
    Some(g)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_filters_are_scipys() {
        // scipy.signal.butter(1, 7, fs=44100) and butter(1, 800 / 3000, fs=44100), SciPy 1.17.1.
        let (b0, b1, a1) = butter1(7.0, 44_100);
        assert!((b0 - 0.000_498_416_998_521_082_4).abs() < 1e-17, "{b0}");
        assert!((b1 - b0).abs() < 1e-18);
        assert!((a1 + 0.999_003_166_002_957_9).abs() < 1e-15, "{a1}");
        let (_, _, a1) = butter1(800.0 / 3000.0, 44_100);
        assert!((a1 + 0.999_962_007_159_788_9).abs() < 1e-15, "{a1}");
        // The steady state carries a constant straight through filtfilt.
        let y = filtfilt1(0.25, 0.0, -0.75, &[0.5; 50]);
        assert!(y.iter().all(|v| (v - 0.5).abs() < 1e-12));
    }

    #[test]
    fn the_max_filter_matches_a_plain_window() {
        let x: Vec<f64> = (0..300).map(|i| ((i * 37 % 101) as f64).sin()).collect();
        for (b, a) in [(44, 44), (43, 0), (3, 7)] {
            let got = max_filter(&x, b, a);
            for i in 0..x.len() {
                let lo = i.saturating_sub(b);
                let hi = (i + a).min(x.len() - 1);
                let want = x[lo..=hi].iter().cloned().fold(f64::MIN, f64::max);
                assert_eq!(got[i], want);
            }
        }
    }

    #[test]
    fn under_the_ceiling_nothing_changes() {
        assert!(gain(&[0.1, 0.5, 0.9], 0.99, 44_100, &LimiterConfig::default()).is_none());
    }
}
