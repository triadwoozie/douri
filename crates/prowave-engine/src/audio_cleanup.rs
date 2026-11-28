//! Audio cleanup & analysis DSP ported from Audacity (GPLv2/v3) and aubio (GPLv3).
//!
//! - Audacity-derived offline cleanup: `remove_dc_offset`, `click_removal`, `noise_reduction_spectral`.
//! - aubio-derived analysis: `aubio_spectral_flux_onsets`, `aubio_yin_fast_pitch`.

use std::f32::consts::PI;

/// Removes DC offset from stereo slices in-place (Audacity `RemoveDcOffset` style).
/// Allocation-free.
pub fn remove_dc_offset(left: &mut [f32], right: &mut [f32]) {
    for ch in [left, right] {
        if ch.is_empty() {
            continue;
        }
        let sum: f64 = ch.iter().map(|&s| f64::from(s)).sum();
        let mean = (sum / ch.len() as f64) as f32;
        if mean.is_finite() && mean != 0.0 {
            for s in ch.iter_mut() {
                *s -= mean;
            }
        }
    }
}

/// Repairs narrow impulsive clicks in-place using local RMS spike detection and linear
/// interpolation across detected spans (Audacity `ClickRemoval` style).
pub fn click_removal(samples: &mut [f32], threshold: f32, max_width: usize) {
    let n = samples.len();
    let max_w = max_width.clamp(2, 64);
    if n < max_w * 4 {
        return;
    }
    let thresh = threshold.clamp(1.5, 20.0);
    let win = 64usize;
    let orig = samples.to_vec();
    let mut i = win;
    while i + win < n {
        // Local background RMS excluding the immediate center span.
        let mut bg_sq = 0.0f64;
        let mut bg_cnt = 0usize;
        for &sample in &orig[(i - win)..(i - 2)] {
            let v = f64::from(sample);
            bg_sq += v * v;
            bg_cnt += 1;
        }
        let right_start = (i + max_w + 2).min(n);
        let right_end = (i + win).min(n);
        for &sample in &orig[right_start..right_end] {
            let v = f64::from(sample);
            bg_sq += v * v;
            bg_cnt += 1;
        }
        let bg_rms = (bg_sq / bg_cnt.max(1) as f64).sqrt().max(1e-5) as f32;
        if orig[i].abs() > bg_rms * thresh {
            let mut end = i + 1;
            while end < (i + max_w).min(n - 1) && orig[end].abs() > bg_rms * (thresh * 0.65) {
                end += 1;
            }
            let a = samples[i.saturating_sub(1)];
            let b = orig[end.min(n - 1)];
            let span = (end - i + 1) as f32;
            for (k, s) in samples[i..end].iter_mut().enumerate() {
                let t = (k + 1) as f32 / span;
                *s = a + (b - a) * t;
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
}

/// Spectral noise reduction (Audacity `NoiseReduction` style): estimates noise floor per
/// frequency bin from the quietest STFT frames and applies smooth Wiener-like attenuation.
pub fn noise_reduction_spectral(
    samples: &mut [f32],
    sample_rate: u32,
    reduction_db: f32,
    sensitivity: f32,
) {
    let n = samples.len();
    const FFT_N: usize = 1024;
    const HOP: usize = 256;
    if n < FFT_N || sample_rate == 0 {
        return;
    }
    let bins = FFT_N / 2 + 1;
    let hann: Vec<f32> = (0..FFT_N)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / FFT_N as f32).cos()))
        .collect();
    let num_frames = (n - FFT_N) / HOP + 1;
    if num_frames == 0 {
        return;
    }

    // Simple radix-2 DFT/IDFT using precomputed twiddles for 1024 points.
    let mut frame_mags = vec![0.0f32; num_frames * bins];
    let mut frame_re = vec![0.0f32; num_frames * FFT_N];
    let mut frame_im = vec![0.0f32; num_frames * FFT_N];
    let mut frame_energy = Vec::with_capacity(num_frames);

    let mut re = vec![0.0f32; FFT_N];
    let mut im = vec![0.0f32; FFT_N];
    for t in 0..num_frames {
        let start = t * HOP;
        let mut e = 0.0f32;
        for i in 0..FFT_N {
            let v = samples[start + i] * hann[i];
            re[i] = v;
            im[i] = 0.0;
            e += v * v;
        }
        frame_energy.push((e, t));
        fft_in_place(&mut re, &mut im, false);
        frame_re[t * FFT_N..(t + 1) * FFT_N].copy_from_slice(&re);
        frame_im[t * FFT_N..(t + 1) * FFT_N].copy_from_slice(&im);
        for k in 0..bins {
            frame_mags[t * bins + k] = (re[k] * re[k] + im[k] * im[k]).sqrt();
        }
    }

    // Estimate noise profile from the quietest 15% of frames.
    frame_energy.sort_by(|a, b| a.0.total_cmp(&b.0));
    let quiet_cnt = (num_frames / 7).max(1).min(num_frames);
    let mut noise_floor = vec![0.0f32; bins];
    for &(_, t) in &frame_energy[..quiet_cnt] {
        for k in 0..bins {
            noise_floor[k] = noise_floor[k].max(frame_mags[t * bins + k]);
        }
    }

    let floor_gain = 10f32.powf(-reduction_db.clamp(0.0, 48.0) / 20.0);
    let sens = sensitivity.clamp(0.5, 6.0);

    let mut out = vec![0.0f32; n];
    let mut win_sum = vec![0.0f32; n];
    let inv_n = 1.0 / FFT_N as f32;
    for t in 0..num_frames {
        re.copy_from_slice(&frame_re[t * FFT_N..(t + 1) * FFT_N]);
        im.copy_from_slice(&frame_im[t * FFT_N..(t + 1) * FFT_N]);
        for k in 0..bins {
            let mag = frame_mags[t * bins + k];
            let thresh = noise_floor[k] * sens;
            let gain = if mag >= thresh {
                1.0
            } else {
                let ratio = (mag / thresh.max(1e-9)).clamp(0.0, 1.0);
                floor_gain + (1.0 - floor_gain) * ratio * ratio
            };
            re[k] *= gain;
            im[k] *= gain;
            if k > 0 && k < FFT_N / 2 {
                re[FFT_N - k] *= gain;
                im[FFT_N - k] *= gain;
            }
        }
        fft_in_place(&mut re, &mut im, true);
        let start = t * HOP;
        for i in 0..FFT_N {
            out[start + i] += re[i] * inv_n * hann[i];
            win_sum[start + i] += hann[i] * hann[i];
        }
    }

    for i in 0..n {
        if win_sum[i] > 1e-6 {
            samples[i] = out[i] / win_sum[i];
        }
    }
}

/// aubio-style spectral-flux onset detector with adaptive median peak picking (`onset.c` / `peakpicker.c`).
pub fn aubio_spectral_flux_onsets(mono: &[f32], sample_rate: u32, threshold: f32) -> Vec<f64> {
    const FFT_N: usize = 512;
    const HOP: usize = 256;
    if mono.len() < FFT_N || sample_rate == 0 {
        return Vec::new();
    }
    let bins = FFT_N / 2 + 1;
    let hann: Vec<f32> = (0..FFT_N)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / FFT_N as f32).cos()))
        .collect();
    let num_frames = (mono.len() - FFT_N) / HOP + 1;
    let mut prev_mag = vec![0.0f32; bins];
    let mut flux = Vec::with_capacity(num_frames);
    let mut re = vec![0.0f32; FFT_N];
    let mut im = vec![0.0f32; FFT_N];

    for t in 0..num_frames {
        let start = t * HOP;
        for i in 0..FFT_N {
            re[i] = mono[start + i] * hann[i];
            im[i] = 0.0;
        }
        fft_in_place(&mut re, &mut im, false);
        let mut f = 0.0f32;
        for k in 0..bins {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            let diff = (mag - prev_mag[k]).max(0.0);
            f += diff;
            prev_mag[k] = mag;
        }
        flux.push(f);
    }

    // Adaptive peak-picking over a 7-frame neighborhood.
    let mut onsets = Vec::new();
    let min_gap_frames = ((0.05 * sample_rate as f32) / HOP as f32).round() as usize;
    let mut last_onset = 0usize;
    let thresh = threshold.max(0.05);
    for t in 1..flux.len().saturating_sub(1) {
        if flux[t] <= flux[t - 1] || flux[t] < flux[t + 1] {
            continue;
        }
        let a = t.saturating_sub(3);
        let b = (t + 4).min(flux.len());
        let mean: f32 = flux[a..b].iter().sum::<f32>() / (b - a).max(1) as f32;
        if flux[t] > mean * (1.0 + thresh)
            && (onsets.is_empty() || t >= last_onset + min_gap_frames)
        {
            onsets.push((t * HOP) as f64 / f64::from(sample_rate));
            last_onset = t;
        }
    }
    onsets
}

/// aubio-style YIN fast pitch estimator (`pitchyinfast.c`): returns `(hz, confidence)` in `0..1`.
pub fn aubio_yin_fast_pitch(frame: &[f32], sample_rate: u32, threshold: f32) -> Option<(f32, f32)> {
    let n = frame.len();
    if n < 128 || sample_rate == 0 {
        return None;
    }
    let half = n / 2;
    let mut cmnd = vec![1.0f32; half];
    let mut running = 0.0f32;
    let min_tau = (sample_rate as usize / 1200).max(2);
    let max_tau = (sample_rate as usize / 50).min(half - 2);
    if min_tau >= max_tau {
        return None;
    }
    for tau in 1..=max_tau {
        let mut diff = 0.0f32;
        for j in 0..half {
            let d = frame[j] - frame[j + tau];
            diff += d * d;
        }
        running += diff;
        cmnd[tau] = if running > 0.0 {
            diff * tau as f32 / running
        } else {
            1.0
        };
    }

    let mut best_tau = None;
    for tau in min_tau..max_tau {
        if cmnd[tau] < threshold && cmnd[tau] <= cmnd[tau + 1] {
            let mut t = tau;
            while t + 1 < max_tau && cmnd[t + 1] < cmnd[t] {
                t += 1;
            }
            best_tau = Some(t);
            break;
        }
    }
    let tau = best_tau?;
    let a = cmnd[tau - 1];
    let b = cmnd[tau];
    let c = cmnd[tau + 1];
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > 1e-9 {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let period = tau as f32 + shift;
    let hz = sample_rate as f32 / period;
    let conf = (1.0 - b).clamp(0.0, 1.0);
    Some((hz, conf))
}

fn fft_in_place(re: &mut [f32], im: &mut [f32], inverse: bool) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2usize;
    let sign = if inverse { 1.0f32 } else { -1.0f32 };
    while len <= n {
        let ang = sign * 2.0 * PI / len as f32;
        let (wlen_re, wlen_im) = (ang.cos(), ang.sin());
        for i in (0..n).step_by(len) {
            let (mut w_re, mut w_im) = (1.0f32, 0.0f32);
            for k in 0..(len / 2) {
                let u_re = re[i + k];
                let u_im = im[i + k];
                let v_re = re[i + k + len / 2] * w_re - im[i + k + len / 2] * w_im;
                let v_im = re[i + k + len / 2] * w_im + im[i + k + len / 2] * w_re;
                re[i + k] = u_re + v_re;
                im[i + k] = u_im + v_im;
                re[i + k + len / 2] = u_re - v_re;
                im[i + k + len / 2] = u_im - v_im;
                let next_re = w_re * wlen_re - w_im * wlen_im;
                w_im = w_re * wlen_im + w_im * wlen_re;
                w_re = next_re;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_dc_offset_centers_signal_around_zero() {
        let mut l = vec![0.5f32; 1024];
        let mut r = vec![-0.25f32; 1024];
        l[0] += 0.1;
        l[1] -= 0.1;
        remove_dc_offset(&mut l, &mut r);
        let mean_l: f32 = l.iter().sum::<f32>() / l.len() as f32;
        let mean_r: f32 = r.iter().sum::<f32>() / r.len() as f32;
        assert!(mean_l.abs() < 1e-5);
        assert!(mean_r.abs() < 1e-5);
    }

    #[test]
    fn click_removal_attenuates_impulse_spike() {
        let mut buf: Vec<f32> = (0..1024)
            .map(|i| 0.05 * (2.0 * PI * 220.0 * i as f32 / 48_000.0).sin())
            .collect();
        buf[500] = 0.95;
        buf[501] = -0.90;
        click_removal(&mut buf, 4.0, 8);
        assert!(buf[500].abs() < 0.15, "click remained: {}", buf[500]);
    }

    #[test]
    fn spectral_noise_reduction_lowers_hiss_floor() {
        let mut buf: Vec<f32> = (0..8192)
            .map(|i| {
                let tone = if i > 4096 {
                    0.5 * (2.0 * PI * 440.0 * i as f32 / 48_000.0).sin()
                } else {
                    0.0
                };
                let hiss = 0.04 * ((i * 7919) % 97) as f32 / 97.0 - 0.02;
                tone + hiss
            })
            .collect();
        let before: f32 = buf[512..2560].iter().map(|x| x * x).sum();
        noise_reduction_spectral(&mut buf, 48_000, 24.0, 1.5);
        let after: f32 = buf[512..2560].iter().map(|x| x * x).sum();
        assert!(after < before * 0.4, "before={before}, after={after}");
    }

    #[test]
    fn aubio_yin_fast_and_onset_detect_accurately() {
        let sr = 48_000u32;
        let frame: Vec<f32> = (0..2048)
            .map(|i| (2.0 * PI * 220.0 * i as f32 / sr as f32).sin())
            .collect();
        let Some((hz, conf)) = aubio_yin_fast_pitch(&frame, sr, 0.15) else {
            panic!("expected pitch");
        };
        assert!((hz - 220.0).abs() < 2.0, "got {hz}");
        assert!(conf > 0.85);

        let mut hits = vec![0.0f32; 24_000];
        for i in 0..400 {
            hits[4800 + i] = 0.8 * (2.0 * PI * 1000.0 * i as f32 / sr as f32).sin();
            hits[14400 + i] = 0.8 * (2.0 * PI * 1000.0 * i as f32 / sr as f32).sin();
        }
        let onsets = aubio_spectral_flux_onsets(&hits, sr, 0.5);
        assert!(onsets.len() >= 2, "expected >=2 onsets, got {onsets:?}");
    }
}
