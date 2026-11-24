//! Sample-rate conversion as Matchering does it: `resampy.resample(x, sr_orig, sr_new)` with
//! its `kaiser_best` filter (`resampy/interpn.py`, ported). resampy ships that filter as a
//! table; it is a Kaiser-windowed sinc that is rebuilt here from its parameters (recovered
//! from the shipped table, which they reproduce to 2e-16).
#![allow(clippy::needless_range_loop)]

use super::loudness::bessel_i0;
use std::f64::consts::PI;
use std::sync::OnceLock;

/// `kaiser_best`: zero crossings kept, table points per crossing, Kaiser beta, roll-off.
const NUM_ZEROS: usize = 50;
const PRECISION: usize = 8192;
const BETA: f64 = 12.984_585_247_040_012;
const ROLLOFF: f64 = 0.917_347_371_260_876_1;

/// The right half of the interpolation filter.
fn table() -> &'static Vec<f64> {
    static T: OnceLock<Vec<f64>> = OnceLock::new();
    T.get_or_init(|| {
        let n = NUM_ZEROS * PRECISION;
        let i0b = bessel_i0(BETA);
        let step = NUM_ZEROS as f64 / n as f64;
        (0..=n)
            .map(|j| {
                // np.linspace(0, num_zeros, n + 1), np.sinc
                let x = if j == n {
                    NUM_ZEROS as f64
                } else {
                    j as f64 * step
                };
                let s = ROLLOFF * x;
                let sinc = if s == 0.0 {
                    1.0
                } else {
                    (PI * s).sin() / (PI * s)
                };
                // scipy.signal.windows.kaiser(2n + 1, beta)[n:]
                let r = j as f64 / n as f64;
                let w = bessel_i0(BETA * (1.0 - r * r).max(0.0).sqrt()) / i0b;
                ROLLOFF * sinc * w
            })
            .collect()
    })
}

/// `x` (one channel) from `from` Hz to `to` Hz.
pub fn resample(x: &[f64], from: u32, to: u32) -> Vec<f64> {
    if from == to || from == 0 || to == 0 {
        return x.to_vec();
    }
    let ratio = f64::from(to) / f64::from(from);
    let n_out = (x.len() as f64 * f64::from(to) / f64::from(from)) as usize;
    let base = table();
    let scaled: Vec<f64>;
    let win: &[f64] = if ratio < 1.0 {
        scaled = base.iter().map(|v| ratio * v).collect();
        &scaled
    } else {
        base
    };
    let nwin = win.len();
    let mut delta = vec![0.0; nwin];
    for i in 0..nwin - 1 {
        delta[i] = win[i + 1] - win[i];
    }
    let scale = ratio.min(1.0);
    let step_t = 1.0 / ratio;
    let index_step = (scale * PRECISION as f64) as usize;
    let n_orig = x.len();
    let mut y = vec![0.0; n_out];
    for t in 0..n_out {
        let time = t as f64 * step_t;
        let n = time as usize;
        let mut frac = scale * (time - n as f64);
        let mut index_frac = frac * PRECISION as f64;
        let mut offset = index_frac as usize;
        let mut eta = index_frac - offset as f64;
        let i_max = (n + 1).min((nwin - offset) / index_step);
        let mut acc = 0.0;
        for i in 0..i_max {
            let k = offset + i * index_step;
            acc += (win[k] + eta * delta[k]) * x[n - i];
        }
        frac = scale - frac;
        index_frac = frac * PRECISION as f64;
        offset = index_frac as usize;
        eta = index_frac - offset as f64;
        let k_max = (n_orig.saturating_sub(n + 1)).min((nwin - offset) / index_step);
        for k in 0..k_max {
            let j = offset + k * index_step;
            acc += (win[j] + eta * delta[j]) * x[n + k + 1];
        }
        y[t] = acc;
    }
    y
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_table_starts_and_ends_as_resampys() {
        let t = table();
        assert_eq!(t.len(), 409_601);
        // kaiser_best.npz: half_window[0] is the roll-off itself.
        assert_eq!(t[0], ROLLOFF);
        assert!(t[t.len() - 1].abs() < 1e-6);
    }

    #[test]
    fn a_sine_survives_48k_to_44k1() {
        let x: Vec<f64> = (0..48_000)
            .map(|i| (2.0 * PI * 1000.0 * i as f64 / 48_000.0).sin())
            .collect();
        let y = resample(&x, 48_000, 44_100);
        assert_eq!(y.len(), 44_100);
        for i in 2000..42_000 {
            let want = (2.0 * PI * 1000.0 * i as f64 / 44_100.0).sin();
            assert!((y[i] - want).abs() < 1e-4, "{i}: {} vs {want}", y[i]);
        }
    }
}
