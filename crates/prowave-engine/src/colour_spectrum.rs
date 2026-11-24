//! The colour spectrogram: a port of dadaspec (dadabots, <https://github.com/dada-bots/dadaspec>,
//! `dadaspec.py` at commit 983e8df, 2026-10-30) to Rust. Song sections show up as changes of
//! colour, so the shape of a whole song can be seen at a glance.
//!
//! The method, as dadaspec has it: the audio is resampled to 32 kHz (linear, sample for sample
//! as dadaspec does it) and cut into 2048-sample Hann frames 2048 samples apart (64 ms, not
//! centred, so no frame holds invented padding). Each frame's power spectrum goes through a
//! 30-band Slaney mel filterbank up to 16 kHz, per channel. Then:
//! - **Brightness** is each mel band's level in dB below the loudest band of the whole channel,
//!   the top 60 dB of it, raised to the power 0.6.
//! - **Colour** is which part of the spectrum holds the energy: the mel bands below 200 Hz are
//!   red, 200 Hz to 1.5 kHz green, 1.5 to 16 kHz blue. Each of the three is its summed power in
//!   dB, the bottom cut at -20 dB, scaled so its loudest frame is 1: each is normalised on its
//!   own, so a quiet passage still shows its colour.
//! - A pixel is brightness times colour. The top half of the picture is the left channel with
//!   the bass at the bottom, the bottom half the right channel with the bass at the top: the
//!   halves mirror around the centre line, so a stereo difference shows as asymmetry.
//!
//! What is not ported: decoding (Pro Wave's own decoder reads the file) and the final resize to
//! a thumbnail (the picture is drawn at whatever size the screen gives it).
//!
//! dadaspec's licence:
//!
//! MIT License
//!
//! Copyright (c) 2026 dadabots
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//!
//! Everything here allocates and takes a while on a long file: run it off the audio and
//! interface threads.
use crate::spectral::Fft;

/// The rate the audio is resampled to before it is measured (dadaspec's `TARGET_SR`).
pub const COLOUR_RATE: u32 = 32_000;
/// Samples in one frame, and between frames (dadaspec: `n_fft` = `hop_length` = 2048).
pub const COLOUR_HOP: usize = 2048;
const N_FFT: usize = 2048;
/// Mel bands per channel (dadaspec's `SPEC_MELS`): the picture has twice as many rows.
pub const COLOUR_MELS: usize = 30;
/// The top of the filterbank, in Hz.
const FMAX: f64 = 16_000.0;
/// The three bands and the colour each one is: bass red, mid green, high blue.
const BANDS: [(f64, f64); 3] = [(0.0, 200.0), (200.0, 1500.0), (1500.0, 16_000.0)];

/// The picture of one file: `rows` (= 2 × [`COLOUR_MELS`]) from the top, `frames` columns, one
/// column every [`COLOUR_HOP`] samples at [`COLOUR_RATE`] (64 ms). Column `i` is the sound from
/// `i` × 64 ms to (`i` + 1) × 64 ms into the file; the last part of the file shorter than a
/// frame has no column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColourSpectrum {
    pub frames: usize,
    pub rows: usize,
    /// RGB, row by row from the top (`rows` × `frames` × 3 bytes).
    pub rgb: Vec<u8>,
}

impl ColourSpectrum {
    /// Seconds of sound in one column.
    pub const fn column_secs() -> f64 {
        COLOUR_HOP as f64 / COLOUR_RATE as f64
    }

    /// The colour at row `row` (0 at the top) of column `frame`.
    pub fn at(&self, row: usize, frame: usize) -> [u8; 3] {
        let i = (row * self.frames + frame) * 3;
        match self.rgb.get(i..i + 3) {
            Some(c) => [c[0], c[1], c[2]],
            None => [0; 3],
        }
    }

    /// The seconds of sound the picture covers.
    pub fn secs(&self) -> f64 {
        self.frames as f64 * Self::column_secs()
    }
}

// ------------------------------------------------------------------ the mel scale (Slaney)

const F_SP: f64 = 200.0 / 3.0;
const MIN_LOG_HZ: f64 = 1000.0;
const MIN_LOG_MEL: f64 = MIN_LOG_HZ / F_SP;

fn logstep() -> f64 {
    6.4f64.ln() / 27.0
}

fn hz_to_mel(hz: f64) -> f64 {
    if hz >= MIN_LOG_HZ {
        MIN_LOG_MEL + (hz.max(MIN_LOG_HZ) / MIN_LOG_HZ).ln() / logstep()
    } else {
        hz / F_SP
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    if mel >= MIN_LOG_MEL {
        MIN_LOG_HZ * (logstep() * (mel - MIN_LOG_MEL)).exp()
    } else {
        F_SP * mel
    }
}

/// `n` values from `a` to `b` inclusive, as `numpy.linspace` makes them.
fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    match n {
        0 => Vec::new(),
        1 => vec![a],
        _ => {
            let step = (b - a) / (n - 1) as f64;
            let mut v: Vec<f64> = (0..n).map(|i| a + i as f64 * step).collect();
            v[n - 1] = b;
            v
        }
    }
}

/// One mel filter: the first FFT bin it reaches and its weights from there.
struct Filter {
    first: usize,
    weights: Vec<f32>,
}

/// dadaspec's `_mel_filterbank`: triangles between mel-spaced points, Slaney area-normalised,
/// kept as the stretch of bins each one is not zero over.
fn filterbank(mels: usize, n_fft: usize, rate: f64, fmax: f64) -> Vec<Filter> {
    let pts: Vec<f64> = linspace(hz_to_mel(0.0), hz_to_mel(fmax), mels + 2)
        .into_iter()
        .map(mel_to_hz)
        .collect();
    let bins = n_fft / 2 + 1;
    let fft_f = linspace(0.0, rate / 2.0, bins);
    (0..mels)
        .map(|i| {
            let (lo, ce, hi) = (pts[i], pts[i + 1], pts[i + 2]);
            let enorm = (2.0 / (hi - lo)) as f32;
            let row: Vec<f32> = fft_f
                .iter()
                .map(|&f| {
                    let left = (f - lo) / (ce - lo).max(1e-10);
                    let right = (hi - f) / (hi - ce).max(1e-10);
                    (left.min(right).max(0.0) as f32) * enorm
                })
                .collect();
            let first = row.iter().position(|w| *w != 0.0).unwrap_or(bins);
            let last = row.iter().rposition(|w| *w != 0.0).map_or(first, |l| l + 1);
            Filter {
                first,
                weights: row.get(first..last).unwrap_or_default().to_vec(),
            }
        })
        .collect()
}

/// dadaspec's `_mel_frequencies`: `n` points evenly spaced on the mel scale from 0 to `fmax`
/// (the frequency each band's colour is decided by).
fn mel_frequencies(n: usize, fmax: f64) -> Vec<f64> {
    linspace(hz_to_mel(0.0), hz_to_mel(fmax), n)
        .into_iter()
        .map(mel_to_hz)
        .collect()
}

// ------------------------------------------------------------------ resampling

/// dadaspec's `resample_linear` (equal to torch's linear interpolation, `align_corners=False`),
/// fed in pieces: output sample `j` is read at input position (`j` + 0.5) × in/out − 0.5,
/// worked out in 32-bit floats as dadaspec does.
struct Resampler {
    n_in: usize,
    n_out: usize,
    ratio: f32,
    /// Input frames not needed any more are dropped from the front: `buf[0]` is input `base`.
    base: usize,
    buf: Vec<[f32; 2]>,
    /// The next output sample.
    next: usize,
}

impl Resampler {
    fn new(rate: u32, n_in: usize) -> Self {
        let n_out =
            (n_in as f64 * f64::from(COLOUR_RATE) / f64::from(rate.max(1))).round() as usize;
        Self {
            n_in,
            n_out,
            ratio: (n_in as f64 / n_out.max(1) as f64) as f32,
            base: 0,
            buf: Vec::new(),
            next: 0,
        }
    }

    /// Where output `j` reads: the two input frames and the weight of the second.
    fn position(&self, j: usize) -> (usize, usize, f32) {
        let src = ((j as f32 + 0.5) * self.ratio - 0.5).max(0.0);
        let last = self.n_in.saturating_sub(1);
        let i0 = (src as usize).min(last);
        let i1 = (i0 + 1).min(last);
        (i0, i1, src - i0 as f32)
    }

    /// Adds input frames (interleaved stereo) and hands every output frame they complete to
    /// `out`. At the end (`done`), frames the file did not have are read as its last one.
    fn push(&mut self, stereo: &[f32], done: bool, out: &mut impl FnMut(f32, f32)) {
        let room = self.n_in.saturating_sub(self.base + self.buf.len());
        self.buf.extend(
            stereo
                .as_chunks::<2>()
                .0
                .iter()
                .take(room)
                .map(|f| [f[0], f[1]]),
        );
        let have = self.base + self.buf.len();
        while self.next < self.n_out {
            let (i0, i1, lam) = self.position(self.next);
            if i1 >= have && !done {
                break;
            }
            let get = |i: usize| {
                let k = i
                    .saturating_sub(self.base)
                    .min(self.buf.len().saturating_sub(1));
                self.buf.get(k).copied().unwrap_or([0.0; 2])
            };
            let (a, b) = (get(i0), get(i1));
            out(
                (1.0 - lam) * a[0] + lam * b[0],
                (1.0 - lam) * a[1] + lam * b[1],
            );
            self.next += 1;
        }
        // Keep from the first frame the next output can read.
        if self.next < self.n_out {
            let (i0, _, _) = self.position(self.next);
            let drop = i0.saturating_sub(self.base).min(self.buf.len());
            if drop > 4096 {
                self.buf.drain(..drop);
                self.base += drop;
            }
        }
    }
}

// ------------------------------------------------------------------ the analysis

/// Measures a file's colour spectrogram from its samples, fed in pieces of interleaved stereo
/// at the file's rate ([`ColourAnalyser::push`]), then [`ColourAnalyser::finish`]. Holds the
/// mel levels (about 15 KB a second of sound), never the sound.
pub struct ColourAnalyser {
    /// `None` when the file is already at [`COLOUR_RATE`].
    resampler: Option<Resampler>,
    fft: Fft,
    window: Vec<f32>,
    filters: Vec<Filter>,
    /// The frame being filled, per channel.
    frame: [Vec<f32>; 2],
    /// Samples at 32 kHz so far.
    samples: usize,
    /// Mel power per frame, per channel (`frames` × `COLOUR_MELS`).
    mel: [Vec<f32>; 2],
    re: Vec<f32>,
    im: Vec<f32>,
}

impl ColourAnalyser {
    /// For `frames` stereo frames at `rate` (the length decides the resampling, as in
    /// dadaspec, where the whole file is resampled at once).
    pub fn new(rate: u32, frames: u64) -> Self {
        let resampler = (rate != COLOUR_RATE).then(|| Resampler::new(rate, frames as usize));
        let fft = Fft::new(N_FFT);
        let n = fft.len();
        // Periodic Hann, as dadaspec's `_stft_power_np`.
        let window = (0..n)
            .map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos()) as f32)
            .collect();
        Self {
            resampler,
            fft,
            window,
            filters: filterbank(COLOUR_MELS, N_FFT, f64::from(COLOUR_RATE), FMAX),
            frame: [Vec::with_capacity(N_FFT), Vec::with_capacity(N_FFT)],
            samples: 0,
            mel: [Vec::new(), Vec::new()],
            re: vec![0.0; n],
            im: vec![0.0; n],
        }
    }

    /// Adds interleaved stereo samples at the file's rate.
    pub fn push(&mut self, stereo: &[f32]) {
        self.feed(stereo, false);
    }

    fn feed(&mut self, stereo: &[f32], done: bool) {
        match self.resampler.take() {
            Some(mut r) => {
                r.push(stereo, done, &mut |l, rr| self.sample(l, rr));
                self.resampler = Some(r);
            }
            None => {
                for f in stereo.as_chunks::<2>().0 {
                    self.sample(f[0], f[1]);
                }
            }
        }
    }

    /// One sample at 32 kHz.
    fn sample(&mut self, l: f32, r: f32) {
        self.frame[0].push(l);
        self.frame[1].push(r);
        self.samples += 1;
        if self.frame[0].len() == N_FFT {
            self.measure_frame();
        }
    }

    /// The mel power of the frame filled: both channels in one complex transform (left real,
    /// right imaginary), parted again by the symmetry of a real signal's spectrum.
    fn measure_frame(&mut self) {
        let n = N_FFT;
        for i in 0..n {
            self.re[i] = self.frame[0].get(i).copied().unwrap_or(0.0) * self.window[i];
            self.im[i] = self.frame[1].get(i).copied().unwrap_or(0.0) * self.window[i];
        }
        self.fft.process(&mut self.re, &mut self.im, false);
        let bins = n / 2 + 1;
        let mut power = [Vec::with_capacity(bins), Vec::with_capacity(bins)];
        for k in 0..bins {
            let j = (n - k) % n;
            let (xr, xi, yr, yi) = (self.re[k], self.im[k], self.re[j], self.im[j]);
            // Left: (X[k] + conj X[n-k]) / 2; right: (X[k] - conj X[n-k]) / 2i.
            let (lr, li) = ((xr + yr) * 0.5, (xi - yi) * 0.5);
            let (rr, ri) = ((xi + yi) * 0.5, (yr - xr) * 0.5);
            power[0].push(lr * lr + li * li);
            power[1].push(rr * rr + ri * ri);
        }
        for (c, p) in power.iter().enumerate() {
            for f in &self.filters {
                let s: f32 = f
                    .weights
                    .iter()
                    .zip(p.get(f.first..).unwrap_or_default())
                    .map(|(w, x)| w * x)
                    .sum();
                self.mel[c].push(s);
            }
        }
        self.frame[0].clear();
        self.frame[1].clear();
    }

    /// The picture.
    pub fn finish(mut self) -> ColourSpectrum {
        self.feed(&[], true);
        // A sound shorter than one frame is padded with silence to fill it, so there is a
        // column; otherwise the part after the last whole frame is left out (dadaspec).
        if self.mel[0].is_empty() {
            self.frame[0].resize(N_FFT, 0.0);
            self.frame[1].resize(N_FFT, 0.0);
            self.measure_frame();
        }
        let frames = self.mel[0].len() / COLOUR_MELS;
        let mels = COLOUR_MELS;
        let rows = mels * 2;
        let shade = [brightness(&self.mel[0]), brightness(&self.mel[1])];
        let tint = [tint(&self.mel[0], frames), tint(&self.mel[1], frames)];
        let mut rgb = vec![0u8; rows * frames * 3];
        for row in 0..rows {
            // Top half: the left channel, flipped so the bass is at the bottom; bottom half:
            // the right channel, bass at the top.
            let (c, m) = if row < mels {
                (0, mels - 1 - row)
            } else {
                (1, row - mels)
            };
            for f in 0..frames {
                let s = shade[c][f * mels + m];
                for (k, t) in tint[c][f].iter().enumerate() {
                    // numpy's `astype(uint8)` cuts the fraction off.
                    rgb[(row * frames + f) * 3 + k] = ((s * t).clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
        }
        ColourSpectrum { frames, rows, rgb }
    }
}

/// dadaspec's brightness: dB below the loudest mel band of the channel (at most 80 dB down),
/// the top 60 dB scaled to 0..1, raised to the power 0.6 for contrast.
fn brightness(mel: &[f32]) -> Vec<f32> {
    let db: Vec<f32> = mel.iter().map(|p| 10.0 * p.max(1e-10).log10()).collect();
    let top = db.iter().copied().fold(f32::MIN, f32::max);
    db.iter()
        .map(|d| {
            let d = (d - top).max(-80.0).clamp(-60.0, 0.0);
            ((d + 60.0) / 60.0).powf(0.6)
        })
        .collect()
}

/// dadaspec's colour of each frame: the summed power of the mel bands in each of the three
/// bands, in dB from -20 up, each scaled so its loudest frame is 1.
fn tint(mel: &[f32], frames: usize) -> Vec<[f32; 3]> {
    let freqs = mel_frequencies(COLOUR_MELS, FMAX);
    let mut out = vec![[0f32; 3]; frames];
    for (b, &(lo, hi)) in BANDS.iter().enumerate() {
        let members: Vec<usize> = (0..COLOUR_MELS)
            .filter(|&m| freqs[m] >= lo && freqs[m] < hi)
            .collect();
        if members.is_empty() {
            continue;
        }
        let db: Vec<f32> = (0..frames)
            .map(|f| {
                let p: f32 = members.iter().map(|&m| mel[f * COLOUR_MELS + m]).sum();
                (10.0 * (p + 1e-10).log10()).max(-20.0) + 20.0
            })
            .collect();
        let top = db.iter().copied().fold(0f32, f32::max);
        for (o, d) in out.iter_mut().zip(&db) {
            o[b] = if top > 0.0 { d / top } else { *d };
        }
    }
    // The colours are pure red, green and blue, so the mix is the three bands as they are;
    // dadaspec then scales each colour so its loudest frame is 1 (already so, kept as it does).
    for c in 0..3 {
        let top = out.iter().map(|o| o[c]).fold(0f32, f32::max);
        if top > 0.0 {
            for o in &mut out {
                o[c] /= top;
            }
        }
    }
    out
}

/// The colour spectrogram of interleaved stereo `stereo` at `rate`.
pub fn colour_spectrum(stereo: &[f32], rate: u32) -> ColourSpectrum {
    let mut a = ColourAnalyser::new(rate, (stereo.len() / 2) as u64);
    a.push(stereo);
    a.finish()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Stereo from a function of time (seconds) giving (left, right).
    fn render(rate: u32, secs: f64, f: impl Fn(f64) -> (f64, f64)) -> Vec<f32> {
        let n = (secs * f64::from(rate)) as usize;
        let mut out = Vec::with_capacity(n * 2);
        for i in 0..n {
            let (l, r) = f(i as f64 / f64::from(rate));
            out.push(l as f32);
            out.push(r as f32);
        }
        out
    }

    fn sine(hz: f64, t: f64) -> f64 {
        (std::f64::consts::TAU * hz * t).sin()
    }

    /// The test signal the golden picture was made from with dadaspec itself
    /// (`tools/dadaspec_golden.py`): 8 s at 44.1 kHz, a bass section then a bright one, the
    /// channels different.
    fn golden_signal() -> Vec<f32> {
        render(44_100, 8.0, |t| {
            let env = 0.6 + 0.4 * sine(2.0, t).abs();
            if t < 4.0 {
                (
                    env * (0.3 * sine(110.0, t) + 0.1 * sine(440.0, t)),
                    env * (0.2 * sine(220.0, t) + 0.05 * sine(3000.0, t)),
                )
            } else {
                (
                    env * (0.2 * sine(2500.0, t) + 0.1 * sine(6000.0, t) + 0.02 * sine(90.0, t)),
                    env * (0.15 * sine(5000.0, t) + 0.1 * sine(800.0, t)),
                )
            }
        })
    }

    /// The picture dadaspec draws of [`golden_signal`] before it is resized: rows × frames × 3
    /// bytes after an 8-byte header (rows, frames as little-endian u32).
    const GOLDEN: &[u8] = include_bytes!("../tests/data/dadaspec-golden.bin");

    #[test]
    fn the_picture_matches_dadaspec_itself() {
        let rows = u32::from_le_bytes(GOLDEN[0..4].try_into().unwrap()) as usize;
        let frames = u32::from_le_bytes(GOLDEN[4..8].try_into().unwrap()) as usize;
        let want = &GOLDEN[8..];
        let got = colour_spectrum(&golden_signal(), 44_100);
        assert_eq!((got.rows, got.frames), (rows, frames));
        assert_eq!(want.len(), got.rgb.len());
        let diffs: Vec<u8> = want
            .iter()
            .zip(&got.rgb)
            .map(|(a, b)| a.abs_diff(*b))
            .collect();
        let max = diffs.iter().copied().max().unwrap();
        let mean = diffs.iter().map(|d| f64::from(*d)).sum::<f64>() / diffs.len() as f64;
        let off = diffs.iter().filter(|d| **d > 1).count();
        println!("against dadaspec: largest difference {max}, mean {mean:.4}, {off} over 1");
        assert!(
            max <= 3 && mean < 0.1 && off * 1000 < diffs.len(),
            "largest difference {max}, mean {mean}, {off} of {} values more than 1 apart",
            diffs.len()
        );
    }

    #[test]
    fn the_analysis_is_deterministic_and_fed_in_any_pieces() {
        let x = golden_signal();
        let whole = colour_spectrum(&x, 44_100);
        assert_eq!(whole, colour_spectrum(&x, 44_100));
        for piece in [2, 1000, 4096 * 2 + 6, 77_778] {
            let mut a = ColourAnalyser::new(44_100, (x.len() / 2) as u64);
            for c in x.chunks(piece) {
                a.push(c);
            }
            assert_eq!(a.finish(), whole, "pieces of {piece}");
        }
    }

    /// The average colour of columns `a..b` over every row.
    fn mean_colour(s: &ColourSpectrum, a: usize, b: usize) -> [f32; 3] {
        let mut sum = [0f32; 3];
        for row in 0..s.rows {
            for f in a..b {
                let c = s.at(row, f);
                for k in 0..3 {
                    sum[k] += f32::from(c[k]);
                }
            }
        }
        sum.map(|v| v / (s.rows * (b - a)) as f32)
    }

    /// The column where the colour changes most between the columns either side.
    fn sharpest_change(s: &ColourSpectrum) -> usize {
        (1..s.frames)
            .max_by(|&a, &b| {
                let d = |f: usize| {
                    let (x, y) = (mean_colour(s, f - 1, f), mean_colour(s, f, f + 1));
                    (0..3).map(|k| (x[k] - y[k]).abs()).sum::<f32>()
                };
                d(a).total_cmp(&d(b))
            })
            .unwrap()
    }

    #[test]
    fn two_sections_are_two_colours_and_the_change_is_where_the_sound_changes() {
        // A bass and kick section, then a hi-hat and lead section, 6 s each, at 48 kHz.
        let x = render(48_000, 12.0, |t| {
            if t < 6.0 {
                let v = 0.5 * sine(55.0, t) + 0.3 * sine(110.0, t);
                (v, v)
            } else {
                let v = 0.2 * sine(3500.0, t) + 0.2 * sine(7000.0, t) + 0.1 * sine(11_000.0, t);
                (v, v)
            }
        });
        let s = colour_spectrum(&x, 48_000);
        let col = ColourSpectrum::column_secs();
        assert_eq!(s.frames, (12.0 * 32_000.0 / 2048.0) as usize);
        let cut = (6.0 / col) as usize;
        let a = mean_colour(&s, 5, cut - 5);
        let b = mean_colour(&s, cut + 5, s.frames - 5);
        assert!(
            a[0] > 4.0 * a[1].max(a[2]) && a[0] > 5.0,
            "bass is red: {a:?}"
        );
        assert!(
            b[2] > 4.0 * b[0].max(b[1]) && b[2] > 5.0,
            "highs are blue: {b:?}"
        );
        // The boundary: in the column holding 6 s (6 / 0.064 = 93.75: column 93).
        let at = sharpest_change(&s);
        assert!(
            (at as f64 * col - 6.0).abs() <= col,
            "the colour changes at column {at} ({} s)",
            at as f64 * col
        );
    }

    #[test]
    fn halves_mirror_for_mono_and_differ_for_stereo() {
        let mono = render(32_000, 3.0, |t| {
            let v = 0.3 * sine(100.0, t) + 0.1 * sine(4000.0, t);
            (v, v)
        });
        let s = colour_spectrum(&mono, 32_000);
        let half = s.rows / 2;
        for f in 0..s.frames {
            for r in 0..half {
                assert_eq!(
                    s.at(half - 1 - r, f),
                    s.at(half + r, f),
                    "row {r} frame {f}"
                );
            }
        }
        // 80 Hz left, 5 kHz right: the top half red, the bottom half blue.
        let stereo = render(32_000, 3.0, |t| {
            (0.3 * sine(80.0, t), 0.3 * sine(5000.0, t))
        });
        let s = colour_spectrum(&stereo, 32_000);
        let (mut top, mut bottom) = ([0f32; 3], [0f32; 3]);
        for f in 0..s.frames {
            for r in 0..half {
                for k in 0..3 {
                    top[k] += f32::from(s.at(r, f)[k]);
                    bottom[k] += f32::from(s.at(half + r, f)[k]);
                }
            }
        }
        assert!(top[0] > 3.0 * top[2], "left is red: {top:?}");
        assert!(bottom[2] > 3.0 * bottom[0], "right is blue: {bottom:?}");
    }

    #[test]
    fn short_and_silent_sounds_still_have_a_column() {
        let s = colour_spectrum(&[0.1; 200], 44_100);
        assert_eq!((s.frames, s.rows), (1, 2 * COLOUR_MELS));
        let s = colour_spectrum(&vec![0.0; 2 * 48_000], 48_000);
        assert!(s.frames > 10);
        assert!(s.rgb.iter().all(|v| *v == 0), "silence is black");
    }

    #[test]
    fn the_resampler_is_dadaspecs() {
        // dadaspec: n_out = round(n_in * 32000 / sr), sample j read at (j + 0.5) * n_in / n_out
        // - 0.5, between the two input samples around it.
        let x: Vec<f32> = (0..1000).flat_map(|i| [i as f32, -(i as f32)]).collect();
        let mut r = Resampler::new(48_000, 1000);
        let mut out = Vec::new();
        r.push(&x, true, &mut |l, rr| out.push((l, rr)));
        assert_eq!(out.len(), 667);
        let ratio = (1000.0f64 / 667.0) as f32;
        for (j, (l, rr)) in out.iter().enumerate() {
            let want = ((j as f32 + 0.5) * ratio - 0.5).clamp(0.0, 999.0);
            assert!((l - want).abs() < 1e-3, "{j}: {l} against {want}");
            assert_eq!(*rr, -l);
        }
    }

    #[test]
    fn the_filterbank_is_slaneys() {
        let fb = filterbank(COLOUR_MELS, N_FFT, 32_000.0, FMAX);
        assert_eq!(fb.len(), COLOUR_MELS);
        // Slaney: every triangle has the same area (2 / width × width / 2 = 1, in Hz × bins).
        let df = 16_000.0 / 1024.0;
        for (i, f) in fb.iter().enumerate().skip(2) {
            let area: f32 = f.weights.iter().sum::<f32>() * df;
            assert!((area - 1.0).abs() < 0.08, "filter {i}: area {area}");
        }
        assert!((hz_to_mel(1000.0) - 15.0).abs() < 1e-9);
        assert!((mel_to_hz(hz_to_mel(4321.0)) - 4321.0).abs() < 1e-6);
    }
}
