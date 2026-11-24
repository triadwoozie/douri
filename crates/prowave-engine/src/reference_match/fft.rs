//! A double-precision FFT (radix 2) and what Matchering does with numpy's and SciPy's: the
//! magnitude spectrum of a block, the inverse of a real spectrum (`numpy.fft.irfft`) and a
//! convolution trimmed to its input's length (`scipy.signal.fftconvolve(..., "same")`).
//!
//! Double precision throughout so the port lands on the Python reference's numbers; the
//! engine's other FFTs (`spectral.rs`, `convolver.rs`) are single precision for playback.
#![allow(clippy::needless_range_loop)]

use std::f64::consts::PI;

/// A complex FFT of one power-of-two size.
pub struct Fft {
    n: usize,
    /// The bit-reversed order.
    rev: Vec<usize>,
    /// `e^(-2πik/n)` for `k < n/2`, each from its own `sin_cos` (no recurrence drift).
    tw: Vec<(f64, f64)>,
}

impl Fft {
    /// `n` must be a power of two (at least 2).
    pub fn new(n: usize) -> Self {
        assert!(
            n >= 2 && n.is_power_of_two(),
            "FFT size {n} is not a power of two"
        );
        let bits = n.trailing_zeros();
        let rev = (0..n)
            .map(|i| i.reverse_bits() >> (usize::BITS - bits))
            .collect();
        let tw = (0..n / 2)
            .map(|k| {
                let (s, c) = (-2.0 * PI * k as f64 / n as f64).sin_cos();
                (c, s)
            })
            .collect();
        Self { n, rev, tw }
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// In place: the forward transform, or the inverse scaled by `1/n`.
    pub fn run(&self, re: &mut [f64], im: &mut [f64], inverse: bool) {
        let n = self.n;
        for i in 0..n {
            let j = self.rev[i];
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let (c, s) = self.tw[k * step];
                    let s = if inverse { -s } else { s };
                    let a = start + k;
                    let b = a + half;
                    let tr = re[b] * c - im[b] * s;
                    let ti = re[b] * s + im[b] * c;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len *= 2;
        }
        if inverse {
            let k = 1.0 / n as f64;
            for i in 0..n {
                re[i] *= k;
                im[i] *= k;
            }
        }
    }
}

/// `numpy.fft.irfft(spectrum)` of a real spectrum of `n/2 + 1` bins: the real signal of `n`
/// samples whose spectrum it is.
pub fn irfft_real(spectrum: &[f64]) -> Vec<f64> {
    let n = (spectrum.len() - 1) * 2;
    let fft = Fft::new(n);
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    re[..spectrum.len()].copy_from_slice(spectrum);
    for k in 1..n / 2 {
        re[n - k] = spectrum[k];
    }
    fft.run(&mut re, &mut im, true);
    re
}

/// `scipy.signal.fftconvolve(x, h, "same")`: the full convolution of `x` with `h`, its middle
/// `x.len()` samples. Done in overlapping blocks (the result is the same as one big FFT to
/// rounding). `progress` hears 0 to 1 and stops it by answering `false` (then `None`).
pub fn convolve_same(
    x: &[f64],
    h: &[f64],
    progress: &mut dyn FnMut(f32) -> bool,
) -> Option<Vec<f64>> {
    let n = x.len();
    let k = h.len();
    let mut out = vec![0.0; n];
    if n == 0 || k == 0 {
        return Some(out);
    }
    // Where "same" starts in the full result (SciPy's `_centered`).
    let skip = (k - 1) / 2;
    let size = (4 * k).next_power_of_two().max(2);
    let block = size - k + 1;
    let fft = Fft::new(size);
    let mut hr = vec![0.0; size];
    let mut hi = vec![0.0; size];
    hr[..k].copy_from_slice(h);
    fft.run(&mut hr, &mut hi, false);
    let mut re = vec![0.0; size];
    let mut im = vec![0.0; size];
    let blocks = n.div_ceil(block);
    for (b, start) in (0..n).step_by(block).enumerate() {
        let end = (start + block).min(n);
        re.iter_mut().for_each(|v| *v = 0.0);
        im.iter_mut().for_each(|v| *v = 0.0);
        re[..end - start].copy_from_slice(&x[start..end]);
        fft.run(&mut re, &mut im, false);
        for i in 0..size {
            let (a, c) = (re[i], im[i]);
            re[i] = a * hr[i] - c * hi[i];
            im[i] = a * hi[i] + c * hr[i];
        }
        fft.run(&mut re, &mut im, true);
        // Full-result sample `start + j` is output sample `start + j - skip`.
        let used = end - start + k - 1;
        for j in 0..used {
            let full = start + j;
            if full >= skip && full - skip < n {
                out[full - skip] += re[j];
            }
        }
        if b % 16 == 0 && !progress(b as f32 / blocks as f32) {
            return None;
        }
    }
    progress(1.0);
    Some(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_fft_matches_a_plain_dft_and_comes_back() {
        let n = 64;
        let x: Vec<f64> = (0..n).map(|i| ((i * 7 % 13) as f64 - 6.0) / 7.0).collect();
        let mut re = x.clone();
        let mut im = vec![0.0; n];
        Fft::new(n).run(&mut re, &mut im, false);
        for k in 0..n {
            let (mut a, mut b) = (0.0, 0.0);
            for (t, v) in x.iter().enumerate() {
                let w = -2.0 * PI * (k * t) as f64 / n as f64;
                a += v * w.cos();
                b += v * w.sin();
            }
            assert!((a - re[k]).abs() < 1e-12 && (b - im[k]).abs() < 1e-12);
        }
        Fft::new(n).run(&mut re, &mut im, true);
        for (a, b) in re.iter().zip(&x) {
            assert!((a - b).abs() < 1e-14);
        }
    }

    #[test]
    fn same_convolution_matches_the_direct_sum() {
        let x: Vec<f64> = (0..5000)
            .map(|i| ((i * 31 % 97) as f64 - 48.0) / 50.0)
            .collect();
        for k in [1usize, 2, 7, 64] {
            let h: Vec<f64> = (0..k).map(|i| 1.0 / (1.0 + i as f64)).collect();
            let got = convolve_same(&x, &h, &mut |_| true).unwrap();
            let skip = (k - 1) / 2;
            for i in (0..x.len()).step_by(37) {
                let full = i + skip;
                let mut want = 0.0;
                for (j, hv) in h.iter().enumerate() {
                    if full >= j && full - j < x.len() {
                        want += hv * x[full - j];
                    }
                }
                assert!((got[i] - want).abs() < 1e-11, "k {k} i {i}");
            }
        }
    }
}
