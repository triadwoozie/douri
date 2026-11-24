//! Measures that are not Matchering's own: integrated loudness (ITU-R BS.1770-4: K-weighting,
//! 400 ms blocks overlapping by 75 %, the -70 LUFS and -10 LU gates) and true peak (the signal
//! between its samples: the band-limited signal at 8 times the rate).
//! Pro Wave shows them, matches the A/B levels with them, and its true-peak ceiling uses them.
#![allow(clippy::needless_range_loop)]

use std::f64::consts::PI;

/// A biquad `b0 b1 b2 / 1 a1 a2`.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

/// The two K-weighting stages for `rate` (libebur128's formulas; at 48 kHz they are the
/// coefficients printed in BS.1770).
fn k_weighting(rate: u32) -> [Biquad; 2] {
    let fs = f64::from(rate.max(1));
    let f0 = 1_681.974_450_955_533;
    let g = 3.999_843_853_973_347;
    let q = 0.707_175_236_955_419_6;
    let k = (PI * f0 / fs).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
        ],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    let f0 = 38.135_470_876_024_44;
    let q = 0.500_327_037_323_877_3;
    let k = (PI * f0 / fs).tan();
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad {
        b: [1.0, -2.0, 1.0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    [shelf, high_pass]
}

fn run(bq: Biquad, x: &[f64]) -> Vec<f64> {
    let (mut z1, mut z2) = (0.0, 0.0);
    x.iter()
        .map(|&v| {
            let y = bq.b[0] * v + z1;
            z1 = bq.b[1] * v - bq.a[0] * y + z2;
            z2 = bq.b[2] * v - bq.a[1] * y;
            y
        })
        .collect()
}

/// Integrated loudness in LUFS of `channels` (each weighted 1, as left and right are) at
/// `rate`. `None` when nothing is above the -70 LUFS gate (or the audio is shorter than a
/// block).
pub fn integrated_lufs(channels: &[&[f64]], rate: u32) -> Option<f64> {
    let n = channels.iter().map(|c| c.len()).min()?;
    let block = (0.4 * f64::from(rate)).round() as usize;
    let hop = (0.1 * f64::from(rate)).round() as usize;
    if n < block || hop == 0 {
        return None;
    }
    let stages = k_weighting(rate);
    let blocks = (n - block) / hop + 1;
    let mut power = vec![0.0; blocks];
    for c in channels {
        let w = run(stages[1], &run(stages[0], &c[..n]));
        // Running sums of squares, so each block is a difference.
        let mut cum = Vec::with_capacity(n + 1);
        cum.push(0.0);
        let mut acc = 0.0;
        for v in &w {
            acc += v * v;
            cum.push(acc);
        }
        for (j, p) in power.iter_mut().enumerate() {
            let s = j * hop;
            *p += (cum[s + block] - cum[s]) / block as f64;
        }
    }
    let lufs = |z: f64| -0.691 + 10.0 * z.log10();
    let above: Vec<f64> = power
        .iter()
        .copied()
        .filter(|&z| z > 0.0 && lufs(z) > -70.0)
        .collect();
    if above.is_empty() {
        return None;
    }
    let relative = lufs(above.iter().sum::<f64>() / above.len() as f64) - 10.0;
    let gated: Vec<f64> = above.into_iter().filter(|&z| lufs(z) > relative).collect();
    if gated.is_empty() {
        return None;
    }
    Some(lufs(gated.iter().sum::<f64>() / gated.len() as f64))
}

/// Oversampling of the true-peak measure.
pub const OVERSAMPLE: usize = 8;
/// Samples per block of the oversampling, and the margin each side that is thrown away.
const BLOCK: usize = 32768;
const MARGIN: usize = 4096;

/// The largest absolute value of `x` between each sample and its neighbours: sample `i`'s entry
/// covers the signal from `i - 1` to `i + 1`. The signal between the samples is the
/// band-limited one (each block's spectrum zero-padded to [`OVERSAMPLE`] times its length), so
/// content right up to the Nyquist frequency is followed. Blocks run on all cores.
pub fn true_peak_envelope(x: &[f64]) -> Vec<f64> {
    let n = x.len();
    let hop = BLOCK - 2 * MARGIN;
    let blocks = n.div_ceil(hop).max(1);
    // between[i]: the largest point strictly between samples i and i + 1.
    let mut between = vec![0.0f64; n];
    let threads = std::thread::available_parallelism()
        .map_or(4, |v| v.get())
        .clamp(1, 16)
        .min(blocks);
    let per = blocks.div_ceil(threads);
    std::thread::scope(|s| {
        for (t, out) in between.chunks_mut(per * hop).enumerate() {
            s.spawn(move || {
                let small = super::fft::Fft::new(BLOCK);
                let big = super::fft::Fft::new(BLOCK * OVERSAMPLE);
                let mut re = vec![0.0; BLOCK];
                let mut im = vec![0.0; BLOCK];
                let mut ure = vec![0.0; BLOCK * OVERSAMPLE];
                let mut uim = vec![0.0; BLOCK * OVERSAMPLE];
                let first = t * per * hop;
                for (b, chunk) in out.chunks_mut(hop).enumerate() {
                    // The block holds samples start .. start + BLOCK, `start` MARGIN before `at`.
                    let at = first + b * hop;
                    for k in 0..BLOCK {
                        let j = at as isize + k as isize - MARGIN as isize;
                        re[k] = if j >= 0 && (j as usize) < n {
                            x[j as usize]
                        } else {
                            0.0
                        };
                        im[k] = 0.0;
                    }
                    small.run(&mut re, &mut im, false);
                    ure.iter_mut().for_each(|v| *v = 0.0);
                    uim.iter_mut().for_each(|v| *v = 0.0);
                    let big_n = BLOCK * OVERSAMPLE;
                    let half = BLOCK / 2;
                    let gain = OVERSAMPLE as f64;
                    ure[..half].copy_from_slice(&re[..half]);
                    uim[..half].copy_from_slice(&im[..half]);
                    for k in 1..half {
                        ure[big_n - k] = re[BLOCK - k];
                        uim[big_n - k] = im[BLOCK - k];
                    }
                    ure[half] = re[half] * 0.5;
                    ure[big_n - half] = re[half] * 0.5;
                    for v in ure.iter_mut().chain(uim.iter_mut()) {
                        *v *= gain;
                    }
                    big.run(&mut ure, &mut uim, true);
                    for (k, e) in chunk.iter_mut().enumerate() {
                        let q0 = (MARGIN + k) * OVERSAMPLE;
                        let mut m = 0.0f64;
                        for q in q0..q0 + OVERSAMPLE {
                            let (a, b, c) = (ure[q - 1].abs(), ure[q].abs(), ure[q + 1].abs());
                            if q > q0 {
                                m = m.max(b);
                            }
                            // A peak between the points: the top of the parabola through them.
                            if b >= a && b >= c {
                                let den = a - 2.0 * b + c;
                                if den < 0.0 {
                                    let p = 0.5 * (a - c) / den;
                                    m = m.max(b - 0.25 * (a - c) * p);
                                }
                            }
                        }
                        *e = m;
                    }
                }
            });
        }
    });
    (0..n)
        .map(|i| {
            let mut m = x[i].abs().max(between[i]);
            if i > 0 {
                m = m.max(between[i - 1]);
            }
            m
        })
        .collect()
}

/// The modified Bessel function I0 (its power series; plenty for the window's arguments).
pub(crate) fn bessel_i0(x: f64) -> f64 {
    let q = x * x / 4.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..200 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < sum * 1e-18 {
            break;
        }
    }
    sum
}

/// The true peak of `channels` (linear).
pub fn true_peak(channels: &[&[f64]]) -> f64 {
    channels
        .iter()
        .map(|c| true_peak_envelope(c).into_iter().fold(0.0, f64::max))
        .fold(0.0, f64::max)
}

/// Linear to dB.
pub fn db(v: f64) -> f64 {
    20.0 * v.max(1e-12).log10()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_k_weighting_at_48k_is_the_standards() {
        let [s, h] = k_weighting(48_000);
        // ITU-R BS.1770-4, tables 1 and 2.
        assert!((s.b[0] - 1.535_124_859_586_97).abs() < 1e-6);
        assert!((s.b[1] + 2.691_696_189_406_38).abs() < 1e-6);
        assert!((s.b[2] - 1.198_392_810_852_85).abs() < 1e-6);
        assert!((s.a[0] + 1.690_659_293_182_41).abs() < 1e-6);
        assert!((s.a[1] - 0.732_480_774_215_85).abs() < 1e-6);
        assert!((h.a[0] + 1.990_047_454_833_98).abs() < 1e-6);
        assert!((h.a[1] - 0.990_072_250_366_21).abs() < 1e-6);
    }

    #[test]
    fn a_full_scale_1k_sine_reads_minus_three_lufs() {
        // BS.1770: a 0 dBFS 997 Hz sine in one channel reads -3.01 LUFS; in two, 0.0.
        let rate = 48_000;
        let x: Vec<f64> = (0..rate as usize * 5)
            .map(|i| (2.0 * PI * 997.0 * i as f64 / f64::from(rate)).sin())
            .collect();
        let one = integrated_lufs(&[&x], rate).unwrap();
        assert!((one + 3.01).abs() < 0.05, "{one}");
        let two = integrated_lufs(&[&x, &x], rate).unwrap();
        assert!(two.abs() < 0.05, "{two}");
    }

    #[test]
    fn true_peak_finds_the_peak_between_samples() {
        // A sine at a quarter of the rate, phased so every sample sits at ±0.707 of its peak.
        let x: Vec<f64> = (0..400)
            .map(|i| (PI / 2.0 * i as f64 + PI / 4.0).sin())
            .collect();
        let sample_peak = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!((sample_peak - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9);
        let env = true_peak_envelope(&x);
        let tp = env[100..300].iter().fold(0.0f64, |m, v| m.max(*v));
        assert!((tp - 1.0).abs() < 0.01, "{tp}");
    }
}
