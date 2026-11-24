//! Reference-matched mastering: a port of Matchering 2.0 (github.com/sergree/matchering,
//! GPL-3.0, Copyright (C) 2016-2022 Sergree) to Rust.
//!
//! A target track is mastered so its loudness, frequency balance, peak level and stereo width
//! follow a reference track:
//! 1. Levels: both are split into mid and side and cut into pieces of at most 15 s; the
//!    pieces louder than the average are "the loudest pieces". The target is turned up or down
//!    so the RMS of its loudest pieces is the reference's.
//! 2. Frequencies: the average magnitude spectrum (4096-point FFTs) of the loudest pieces of
//!    each, divided reference over target, smoothed on a logarithmic frequency grid (LOWESS)
//!    and made into a linear-phase FIR; one for mid, one for side (the side FIR is what moves
//!    the stereo width). The target's mid and side are filtered with them.
//! 3. Level correction: four times, the RMS of the result's loudest pieces (clipped at full
//!    scale) is brought to the reference's again.
//! 4. Finish: Matchering's brickwall limiter (Hyrax), or no limiter with or without peak
//!    normalisation.
//!
//! The order of operations, the constants and the SciPy / numpy / statsmodels / resampy
//! routines it relies on are reproduced so the output lands on the Python reference's
//! (`tests/reference_match_parity.rs` compares the two). Pro Wave adds, beside Matchering's
//! own sample-peak ceiling, a true-peak ceiling ([`Ceiling::TruePeak`]), and integrated
//! loudness readings (`loudness.rs`).
//!
//! Everything runs offline on whole songs (double precision, a few copies of the song in
//! memory: about 2 GB for a 15-minute track); call it from a background thread.
#![allow(clippy::needless_range_loop)]

pub mod fft;
pub mod limiter;
pub mod loudness;
pub mod resample;
pub mod smooth;
pub mod wav;

pub use limiter::LimiterConfig;
pub use wav::{write_wav, Depth};

/// Matchering's `Config` (its defaults). Lengths are in seconds, as Matchering takes them.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub internal_sample_rate: u32,
    pub max_length: f64,
    pub max_piece_size: f64,
    pub threshold: f64,
    pub min_value: f64,
    pub fft_size: usize,
    pub lin_log_oversampling: usize,
    pub rms_correction_steps: usize,
    pub clipping_samples_threshold: usize,
    pub limited_samples_threshold: usize,
    pub allow_equality: bool,
    pub lowess_frac: f64,
    pub lowess_delta: f64,
    pub preview_size: f64,
    pub preview_analysis_step: f64,
    pub preview_fade_size: f64,
    pub preview_fade_coefficient: f64,
    pub limiter: LimiterConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            internal_sample_rate: 44_100,
            max_length: 15.0 * 60.0,
            max_piece_size: 15.0,
            threshold: (32768.0 - 61.0) / 32768.0,
            min_value: 1e-6,
            fft_size: 4096,
            lin_log_oversampling: 4,
            rms_correction_steps: 4,
            clipping_samples_threshold: 8,
            limited_samples_threshold: 128,
            allow_equality: false,
            lowess_frac: 0.0375,
            lowess_delta: 0.001,
            preview_size: 30.0,
            preview_analysis_step: 5.0,
            preview_fade_size: 1.0,
            preview_fade_coefficient: 8.0,
            limiter: LimiterConfig::default(),
        }
    }
}

/// Two channels of the same length.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stereo {
    pub l: Vec<f64>,
    pub r: Vec<f64>,
}

impl Stereo {
    pub fn len(&self) -> usize {
        self.l.len().min(self.r.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// From interleaved stereo.
    pub fn from_interleaved(x: &[f32]) -> Self {
        let n = x.len() / 2;
        let mut l = Vec::with_capacity(n);
        let mut r = Vec::with_capacity(n);
        for [a, b] in x.as_chunks::<2>().0 {
            l.push(f64::from(*a));
            r.push(f64::from(*b));
        }
        Self { l, r }
    }

    /// As interleaved stereo.
    pub fn interleaved(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.len() * 2);
        for i in 0..self.len() {
            out.push(self.l[i] as f32);
            out.push(self.r[i] as f32);
        }
        out
    }

    fn peak(&self) -> f64 {
        self.l
            .iter()
            .chain(&self.r)
            .fold(0.0f64, |m, v| m.max(v.abs()))
    }

    fn scale(&mut self, k: f64) {
        self.l
            .iter_mut()
            .chain(self.r.iter_mut())
            .for_each(|v| *v *= k);
    }

    /// Divided by `k` (Matchering divides where it normalises).
    fn div(&mut self, k: f64) {
        self.l
            .iter_mut()
            .chain(self.r.iter_mut())
            .for_each(|v| *v /= k);
    }
}

/// Audio as it was read: its channels (one or two) at its rate.
#[derive(Clone, Debug)]
pub struct Input {
    pub channels: Vec<Vec<f64>>,
    pub rate: u32,
}

/// Why a match cannot be made.
#[derive(Clone, Debug, PartialEq)]
pub enum MatchError {
    TargetTooLong,
    TargetTooShort,
    ReferenceTooLong,
    ReferenceTooShort,
    /// More than two channels, or none.
    TargetChannels,
    ReferenceChannels,
    TargetEqualsReference,
    Cancelled,
}

impl std::fmt::Display for MatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TargetTooLong => "the target is longer than 15 minutes",
            Self::TargetTooShort => "the target is too short",
            Self::ReferenceTooLong => "the reference is longer than 15 minutes",
            Self::ReferenceTooShort => "the reference is too short",
            Self::TargetChannels => "the target has more than two channels",
            Self::ReferenceChannels => "the reference has more than two channels",
            Self::TargetEqualsReference => "the target and the reference are the same",
            Self::Cancelled => "cancelled",
        })
    }
}

/// What Matchering's checks noticed (its log messages).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    TargetMono,
    ReferenceMono,
    TargetResampled,
    ReferenceResampled,
    TargetClipping,
    TargetLimited,
}

/// `checker.check`: length, channels (mono becomes two equal channels), sample rate.
fn check(
    input: Input,
    cfg: &Config,
    target: bool,
    notices: &mut Vec<Notice>,
) -> Result<Stereo, MatchError> {
    let rate = input.rate.max(1);
    let len = input.channels.first().map_or(0, Vec::len);
    let max = cfg.max_length * f64::from(rate);
    let min = cfg.fft_size * rate as usize / cfg.internal_sample_rate as usize;
    if len as f64 > max {
        return Err(if target {
            MatchError::TargetTooLong
        } else {
            MatchError::ReferenceTooLong
        });
    }
    if len < min {
        return Err(if target {
            MatchError::TargetTooShort
        } else {
            MatchError::ReferenceTooShort
        });
    }
    let mut ch = input.channels;
    let st = match ch.len() {
        1 => {
            notices.push(if target {
                Notice::TargetMono
            } else {
                Notice::ReferenceMono
            });
            let l = ch.remove(0);
            Stereo { r: l.clone(), l }
        }
        2 => {
            let r = ch.remove(1);
            let l = ch.remove(0);
            Stereo { l, r }
        }
        _ => {
            return Err(if target {
                MatchError::TargetChannels
            } else {
                MatchError::ReferenceChannels
            })
        }
    };
    let st = if rate != cfg.internal_sample_rate {
        notices.push(if target {
            Notice::TargetResampled
        } else {
            Notice::ReferenceResampled
        });
        let to = cfg.internal_sample_rate;
        let (l, r) = std::thread::scope(|s| {
            let a = s.spawn(|| resample::resample(&st.l, rate, to));
            let b = resample::resample(&st.r, rate, to);
            (a.join().unwrap_or_default(), b)
        });
        Stereo { l, r }
    } else {
        st
    };
    if target {
        // count_max_peaks
        let max_value = st.peak();
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-8 + 1e-5 * b.abs();
        let count =
            st.l.iter()
                .chain(&st.r)
                .filter(|v| close(**v, max_value) || close(**v, -max_value))
                .count();
        if count > cfg.clipping_samples_threshold {
            if close(max_value, 1.0) {
                notices.push(Notice::TargetClipping);
            } else if count > cfg.limited_samples_threshold {
                notices.push(Notice::TargetLimited);
            }
        }
    }
    Ok(st)
}

/// Checks and converts the target and the reference to Matchering's working form (two
/// channels at the internal rate).
pub fn prepare(
    target: Input,
    reference: Input,
    cfg: &Config,
) -> Result<(Stereo, Stereo, Vec<Notice>), MatchError> {
    let mut notices = Vec::new();
    let t = check(target, cfg, true, &mut notices)?;
    let r = check(reference, cfg, false, &mut notices)?;
    if !cfg.allow_equality && t.len() == r.len() {
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-8 + 1e-5 * b.abs();
        let same = t.l.iter().zip(&r.l).all(|(a, b)| close(*a, *b))
            && t.r.iter().zip(&r.r).all(|(a, b)| close(*a, *b));
        if same {
            return Err(MatchError::TargetEqualsReference);
        }
    }
    if t.len() <= cfg.fft_size {
        return Err(MatchError::TargetTooShort);
    }
    if r.len() <= cfg.fft_size {
        return Err(MatchError::ReferenceTooShort);
    }
    Ok((t, r, notices))
}

/// `dsp.lr_to_ms`, in Matchering's order of operations.
fn lr_to_ms(s: &Stereo) -> (Vec<f64>, Vec<f64>) {
    let n = s.len();
    let mut mid = Vec::with_capacity(n);
    let mut side = Vec::with_capacity(n);
    for i in 0..n {
        let m = (s.l[i] + s.r[i]) * 0.5;
        mid.push(m);
        side.push(m - s.r[i]);
    }
    (mid, side)
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

/// `get_average_rms`: the RMS of each of `divisions` pieces of `piece` samples, and their RMS.
fn piece_rms(x: &[f64], piece: usize, divisions: usize) -> (Vec<f64>, f64) {
    let rmses: Vec<f64> = (0..divisions)
        .map(|d| rms(&x[d * piece..(d + 1) * piece]))
        .collect();
    let avg = rms(&rmses);
    (rmses, avg)
}

/// `get_lpis_and_match_rms`: the pieces at or above the average, and their RMS.
fn loudest(rmses: &[f64], avg: f64) -> (Vec<usize>, f64) {
    let idx: Vec<usize> = (0..rmses.len()).filter(|&i| rmses[i] >= avg).collect();
    let l: Vec<f64> = idx.iter().map(|&i| rmses[i]).collect();
    (idx, rms(&l))
}

/// What `analyze_levels` finds in a song.
struct Levels {
    mid: Vec<f64>,
    side: Vec<f64>,
    loudest: Vec<usize>,
    match_rms: f64,
    divisions: usize,
    piece: usize,
}

fn analyze_levels(s: &Stereo, cfg: &Config) -> Levels {
    let (mid, side) = lr_to_ms(s);
    let size = mid.len();
    let max_piece = cfg.max_piece_size * f64::from(cfg.internal_sample_rate);
    let divisions = (size as f64 / max_piece) as usize + 1;
    let piece = (size as f64 / divisions as f64) as usize;
    let (rmses, avg) = piece_rms(&mid, piece, divisions);
    let (loudest, match_rms) = loudest(&rmses, avg);
    Levels {
        mid,
        side,
        loudest,
        match_rms,
        divisions,
        piece,
    }
}

/// `__average_fft`: the mean magnitude spectrum (`scipy.signal.stft`, boxcar, no overlap, no
/// padding, scaled by the window's sum) over every whole FFT block of the given pieces.
fn average_fft(x: &[f64], pieces: &[usize], piece: usize, n: usize) -> Vec<f64> {
    let f = fft::Fft::new(n);
    let bins = n / 2 + 1;
    let mut sum = vec![0.0; bins];
    let mut count = 0usize;
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    let per = piece / n;
    for &p in pieces {
        for b in 0..per {
            let s = p * piece + b * n;
            re.copy_from_slice(&x[s..s + n]);
            im.iter_mut().for_each(|v| *v = 0.0);
            f.run(&mut re, &mut im, false);
            for k in 0..bins {
                sum[k] += (re[k] * re[k] + im[k] * im[k]).sqrt() / n as f64;
            }
            count += 1;
        }
    }
    let c = count.max(1) as f64;
    sum.iter().map(|v| v / c).collect()
}

/// The matching EQ of one channel (mid or side): its curve and its FIR.
#[derive(Clone, Debug, Default)]
pub struct Eq {
    /// Reference over target per FFT bin, as measured.
    pub raw: Vec<f64>,
    /// After the smoothing: what the FIR does.
    pub smoothed: Vec<f64>,
    pub fir: Vec<f64>,
}

/// `numpy.linspace(start, stop, num)`.
fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut v: Vec<f64> = (0..num).map(|i| i as f64 * step + start).collect();
    v[num - 1] = stop;
    v
}

/// `get_fir`: the EQ that takes the target's loudest pieces to the reference's.
fn get_fir(target: &[f64], t_lv: &Levels, reference: &[f64], r_lv: &Levels, cfg: &Config) -> Eq {
    let n = cfg.fft_size;
    // The target's pieces were turned up by the RMS coefficient: its spectrum scales with it.
    let t_avg = average_fft(target, &t_lv.loudest, t_lv.piece, n);
    let r_avg = average_fft(reference, &r_lv.loudest, r_lv.piece, n);
    let raw: Vec<f64> = r_avg
        .iter()
        .zip(&t_avg)
        .map(|(r, t)| r / t.max(cfg.min_value))
        .collect();
    let smoothed = smooth_exponentially(&raw, cfg);
    let mut fir = fft::irfft_real(&smoothed);
    // ifftshift, then a symmetric Hann window (scipy.signal.windows.hann).
    fir.rotate_left(n / 2);
    let fac = linspace(-std::f64::consts::PI, std::f64::consts::PI, n);
    for (v, a) in fir.iter_mut().zip(&fac) {
        *v *= 0.5 + 0.5 * a.cos();
    }
    Eq { raw, smoothed, fir }
}

/// `__smooth_exponentially`: the curve onto a log-spaced grid (cubic), LOWESS there, and back.
fn smooth_exponentially(matching: &[f64], cfg: &Config) -> Vec<f64> {
    let nyq = f64::from(cfg.internal_sample_rate) * 0.5;
    let n = cfg.fft_size;
    let lin: Vec<f64> = linspace(0.0, 1.0, n / 2 + 1)
        .into_iter()
        .map(|v| nyq * v)
        .collect();
    let log: Vec<f64> = linspace(
        (4.0 / n as f64).log10(),
        0.0,
        (n / 2) * cfg.lin_log_oversampling + 1,
    )
    .into_iter()
    .map(|e| nyq * 10f64.powf(e))
    .collect();
    let up = smooth::Spline::new(&lin, matching);
    let on_log: Vec<f64> = log.iter().map(|&f| up.at(f)).collect();
    let xs = linspace(0.0, 1.0, on_log.len());
    let filtered = smooth::lowess(&on_log, &xs, cfg.lowess_frac, cfg.lowess_delta);
    let down = smooth::Spline::new(&log, &filtered);
    let mut out: Vec<f64> = lin.iter().map(|&f| down.at(f)).collect();
    out[0] = 0.0;
    out[1] = matching[1];
    out
}

/// What the matching found, before the finish: the result without a limiter.
#[derive(Clone, Debug, Default)]
pub struct Matched {
    /// The target matched to the reference, no limiter, no normalisation.
    pub result: Stereo,
    /// What the limited result is multiplied by at the end: below 1 when the reference
    /// peaks under the threshold.
    pub final_amplitude: f64,
    /// The target's first gain (RMS of the loudest pieces).
    pub rms_coefficient: f64,
    /// The gains of the level-correction steps.
    pub corrections: Vec<f64>,
    pub mid: Eq,
    pub side: Eq,
}

/// Progress through a long job: `f` hears 0 to 1 and stops it by answering `false`.
pub type Progress<'a> = &'a mut dyn FnMut(f32) -> bool;

/// A part (`from` to `to`) of the whole job's progress.
fn part<'a>(p: &'a mut dyn FnMut(f32) -> bool, from: f32, to: f32) -> impl FnMut(f32) -> bool + 'a {
    move |x| p(from + (to - from) * x.clamp(0.0, 1.0))
}

/// Matchering's `stages.main` up to the finish (`__match_levels`, `__match_frequencies`,
/// `__correct_levels`), on what [`prepare`] made.
pub fn match_to(
    target: &Stereo,
    reference: &Stereo,
    cfg: &Config,
    progress: Progress<'_>,
) -> Result<Matched, MatchError> {
    let stop = |p: &mut dyn FnMut(f32) -> bool, x: f32| {
        if p(x) {
            Ok(())
        } else {
            Err(MatchError::Cancelled)
        }
    };
    stop(progress, 0.0)?;
    // __match_levels
    let mut reference = reference.clone();
    let ref_peak = reference.peak();
    let final_amplitude = if ref_peak < cfg.threshold {
        let k = cfg.min_value.max(ref_peak / cfg.threshold);
        reference.div(k);
        k
    } else {
        1.0
    };
    let (t_lv, r_lv) = std::thread::scope(|s| {
        let r = s.spawn(|| analyze_levels(&reference, cfg));
        let t = analyze_levels(target, cfg);
        (t, r.join())
    });
    let r_lv = r_lv.map_err(|_| MatchError::Cancelled)?;
    drop(reference);
    stop(progress, 0.08)?;
    let rms_coefficient = r_lv.match_rms / t_lv.match_rms.max(cfg.min_value);
    let mut t_mid = t_lv.mid.clone();
    let mut t_side = t_lv.side.clone();
    t_mid
        .iter_mut()
        .chain(t_side.iter_mut())
        .for_each(|v| *v *= rms_coefficient);
    // __match_frequencies (the loudest pieces amplified the same way)
    let (mid_eq, side_eq) = std::thread::scope(|s| {
        let side = s.spawn(|| get_fir(&t_side, &t_lv, &r_lv.side, &r_lv, cfg));
        let mid = get_fir(&t_mid, &t_lv, &r_lv.mid, &r_lv, cfg);
        (mid, side.join())
    });
    let side_eq = side_eq.map_err(|_| MatchError::Cancelled)?;
    stop(progress, 0.2)?;
    let (r_mid, r_side) = {
        let mut p = part(progress, 0.2, 0.8);
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let (m, s) = std::thread::scope(|s| {
            let side = s.spawn(|| {
                fft::convolve_same(&t_side, &side_eq.fir, &mut |_| {
                    !cancelled.load(std::sync::atomic::Ordering::Relaxed)
                })
            });
            let mid = fft::convolve_same(&t_mid, &mid_eq.fir, &mut |x| {
                let go = p(x);
                if !go {
                    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                go
            });
            (mid, side.join().ok().flatten())
        });
        match (m, s) {
            (Some(m), Some(s)) => (m, s),
            _ => return Err(MatchError::Cancelled),
        }
    };
    drop(t_mid);
    drop(t_side);
    // ms_to_lr
    let mut result = Stereo {
        l: r_mid.iter().zip(&r_side).map(|(m, s)| m + s).collect(),
        r: r_mid.iter().zip(&r_side).map(|(m, s)| m - s).collect(),
    };
    drop(r_side);
    let mut result_mid = r_mid;
    // __correct_levels
    let mut corrections = Vec::new();
    let steps = cfg.rms_correction_steps.max(1) as f32;
    for step in 0..cfg.rms_correction_steps {
        let clipped: Vec<f64> = result_mid
            .iter()
            .take(t_lv.piece * t_lv.divisions)
            .map(|v| v.clamp(-1.0, 1.0))
            .collect();
        let (rmses, avg) = piece_rms(&clipped, t_lv.piece, t_lv.divisions);
        let (_, m) = loudest(&rmses, avg);
        let c = r_lv.match_rms / m.max(cfg.min_value);
        result_mid.iter_mut().for_each(|v| *v *= c);
        result.scale(c);
        corrections.push(c);
        stop(progress, 0.8 + 0.2 * (step as f32 + 1.0) / steps)?;
    }
    Ok(Matched {
        result,
        final_amplitude,
        rms_coefficient,
        corrections,
        mid: mid_eq,
        side: side_eq,
    })
}

/// The ceiling of the finish.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ceiling {
    /// Matchering's: sample peaks at its threshold (-0.016 dBFS).
    Matchering,
    /// True peak at this many dB (dBTP), below 0.
    TruePeak(f64),
}

/// How the result is finished (Matchering's `Result` options).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finish {
    pub limiter: bool,
    /// Without the limiter: peaks brought to the ceiling (Matchering's `normalize`).
    pub normalize: bool,
    pub ceiling: Ceiling,
}

impl Default for Finish {
    fn default() -> Self {
        Self {
            limiter: true,
            normalize: true,
            ceiling: Ceiling::Matchering,
        }
    }
}

/// Each sample's level for the limiter: its peak over the channels, or its true peak.
fn levels_of(s: &Stereo, true_peak: bool) -> Vec<f64> {
    if true_peak {
        let (l, r) = std::thread::scope(|sc| {
            let a = sc.spawn(|| loudness::true_peak_envelope(&s.l));
            let b = loudness::true_peak_envelope(&s.r);
            (a.join().unwrap_or_default(), b)
        });
        l.iter().zip(&r).map(|(a, b)| a.max(*b)).collect()
    } else {
        s.l.iter()
            .zip(&s.r)
            .map(|(a, b)| a.abs().max(b.abs()))
            .collect()
    }
}

/// `__finalize` for one finish.
pub fn finish(
    m: &Matched,
    how: &Finish,
    cfg: &Config,
    progress: Progress<'_>,
) -> Result<Stereo, MatchError> {
    let (threshold, tp) = match how.ceiling {
        Ceiling::Matchering => (cfg.threshold, false),
        Ceiling::TruePeak(db) => (10f64.powf(db.min(0.0) / 20.0), true),
    };
    let mut out = m.result.clone();
    if !progress(0.05) {
        return Err(MatchError::Cancelled);
    }
    if how.limiter {
        let level = levels_of(&out, tp);
        if !progress(0.4) {
            return Err(MatchError::Cancelled);
        }
        if let Some(g) = limiter::gain(&level, threshold, cfg.internal_sample_rate, &cfg.limiter) {
            for i in 0..out.len() {
                out.l[i] *= g[i];
                out.r[i] *= g[i];
            }
        }
        if tp {
            // The gain moves between samples too; a last trim keeps the true peak at the ceiling.
            if !progress(0.75) {
                return Err(MatchError::Cancelled);
            }
            let peak = levels_of(&out, true).into_iter().fold(0.0, f64::max);
            if peak > threshold {
                out.scale(threshold / peak);
            }
        }
        out.scale(m.final_amplitude);
    } else if how.normalize {
        let peak = if tp {
            levels_of(&out, true).into_iter().fold(0.0, f64::max)
        } else {
            out.peak()
        };
        let k = cfg.min_value.max(peak / threshold);
        out.div(k);
    }
    progress(1.0);
    Ok(out)
}

/// Matchering's whole `process` on audio in memory: checks, matching, finish.
pub fn process(
    target: Input,
    reference: Input,
    cfg: &Config,
    how: &Finish,
    progress: Progress<'_>,
) -> Result<(Stereo, Vec<Notice>), MatchError> {
    let (t, r, notices) = prepare(target, reference, cfg)?;
    let m = {
        let mut p = part(progress, 0.0, 0.85);
        match_to(&t, &r, cfg, &mut p)?
    };
    let mut p = part(progress, 0.85, 1.0);
    Ok((finish(&m, how, cfg, &mut p)?, notices))
}

/// `create_preview`: the loudest `preview_size` seconds of the result (stepping by
/// `preview_analysis_step`), the same stretch of the target (clipped at the threshold), both
/// faded in and out. Returns the target's piece, the result's, and where they start.
pub fn preview(target: &Stereo, result: &Stereo, cfg: &Config) -> (Stereo, Stereo, usize) {
    let sr = f64::from(cfg.internal_sample_rate);
    let size = (cfg.preview_size * sr) as usize;
    let step = ((cfg.preview_analysis_step * sr) as usize).max(1);
    let n = result.len().min(target.len());
    let (start, len) = if size > n {
        (0, n)
    } else {
        let count = (n - size) / step + 1;
        let mut best = (0usize, -1.0f64);
        for c in 0..count {
            let s = c * step;
            let e: f64 = (s..s + size)
                .map(|i| result.l[i] * result.l[i] + result.r[i] * result.r[i])
                .sum();
            let v = (e / (2 * size) as f64).sqrt();
            if v > best.1 {
                best = (c, v);
            }
        }
        (best.0 * step, size)
    };
    let th = cfg.threshold;
    let mut t = Stereo {
        l: target.l[start..start + len]
            .iter()
            .map(|v| v.clamp(-th, th))
            .collect(),
        r: target.r[start..start + len]
            .iter()
            .map(|v| v.clamp(-th, th))
            .collect(),
    };
    let mut r = Stereo {
        l: result.l[start..start + len].to_vec(),
        r: result.r[start..start + len].to_vec(),
    };
    if len != n {
        let fade = ((cfg.preview_fade_size * sr) as usize)
            .min(len / cfg.preview_fade_coefficient as usize);
        if fade > 0 {
            let ramp = linspace(0.0, 1.0, fade);
            for s in [&mut t, &mut r] {
                for i in 0..fade {
                    s.l[i] *= ramp[i];
                    s.r[i] *= ramp[i];
                    let j = len - fade + i;
                    s.l[j] *= ramp[fade - 1 - i];
                    s.r[j] *= ramp[fade - 1 - i];
                }
            }
        }
    }
    (t, r, start)
}
