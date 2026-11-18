//! The 808: a tuned, monophonic 808 bass. The MIDI note sets the pitch; the sound is either a
//! synthesized sine with a falling pitch envelope (the TR-808 kick stretched into a bass) or a
//! one-shot sample played back at the speed that puts its measured fundamental on the note.
//!
//! One voice, as 808 bass is played: a new note cuts the last (a 4 ms fade, no click). With
//! Legato on, a note that overlaps a held one slides to its pitch without starting again;
//! with Legato off every note starts again, and slides from the last pitch when Glide is above
//! zero. Letting go of the sounding note while others are held slides back to the last of them
//! (last-note priority, as on a mono synth). Glide moves linearly in semitones, so a slide
//! takes the same time whatever its interval.
//!
//! Two waves can be mixed in under the voice, each with its own level: a sine and a square
//! (PolyBLEP, so its edges do not alias). They sound at the voice's own pitch (note, Tune,
//! bend, slide and punch) and under its envelope; with the synthesized voice they share its
//! phase.
//!
//! A one-shot ends where its file ends. With Sustain on it does not: from a steady stretch of
//! its tail on, a loop made of that stretch plays instead ([`SustainLoop`]), held at the level
//! the one-shot had there, and Decay alone says how long the note lasts.
//!
//! After the voice: a tanh drive (`tanh(k x) / tanh(k)`, unity at full scale), a high-pass
//! filter (24 dB per octave, Butterworth; out of the path when Off), then level.
use crate::plugin_track::BlockProcessor;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// MIDI events one block holds.
const MAX_EVENTS: usize = 512;
/// Notes the held-note stack remembers.
const HELD: usize = 16;
/// The fade of a cut voice, in seconds.
const CUT_SECONDS: f32 = 0.004;
/// The synth voice's attack ramp, in seconds (keeps the sine's start from clicking).
const ATTACK_SECONDS: f32 = 0.001;
/// Below this envelope level a released voice ends.
const SILENT: f32 = 1e-5;
/// Pitch bend range, in semitones each way.
const BEND_SEMITONES: f32 = 2.0;
/// Fundamentals the measurement looks between, in Hz.
const ROOT_LO: f32 = 25.0;
const ROOT_HI: f32 = 140.0;
/// The high-pass filter is off at (and below) this cutoff, the bottom of its range.
const HIGH_PASS_OFF: f32 = BASS808_PARAMS[BASS808_HIGH_PASS].lo;
/// Seconds a wave's level, the filter's cutoff and the filter coming in or out take to follow
/// a change (so automation does not zipper or click).
const SMOOTH_SECONDS: f32 = 0.01;
/// The sustain loop's length, in seconds (at least [`LOOP_PERIODS`] periods).
const LOOP_SECONDS: f64 = 0.25;
const LOOP_PERIODS: usize = 8;
/// The loop lies where the one-shot is still above this part of its loudest period.
const LOOP_FLOOR: f64 = 0.1;
/// A loop's two ends must be at least this alike (correlation), or no loop is made: a poorer
/// match would be heard as a flutter once a turn.
const LOOP_LIKENESS: f64 = 0.9;
/// One-shots kept read and analysed (the last ones used), so a rebuilt 808 does not read and
/// measure its one-shot again.
const PREPARED_KEPT: usize = 8;

/// Automation parameters, by index (`set_parameter`).
pub const BASS808_DECAY: usize = 0;
pub const BASS808_RELEASE: usize = 1;
pub const BASS808_GLIDE: usize = 2;
pub const BASS808_DRIVE: usize = 3;
pub const BASS808_PUNCH: usize = 4;
pub const BASS808_PUNCH_TIME: usize = 5;
pub const BASS808_TUNE: usize = 6;
pub const BASS808_LEVEL: usize = 7;
pub const BASS808_LEGATO: usize = 8;
pub const BASS808_SINE: usize = 9;
pub const BASS808_SQUARE: usize = 10;
pub const BASS808_HIGH_PASS: usize = 11;
pub const BASS808_SUSTAIN: usize = 12;

/// A parameter's name, range and unit (for automation and the window).
#[derive(Clone, Copy, Debug)]
pub struct Bass808Param {
    pub name: &'static str,
    pub lo: f32,
    pub hi: f32,
    pub unit: &'static str,
    pub stepped: bool,
}

const fn param(name: &'static str, lo: f32, hi: f32, unit: &'static str) -> Bass808Param {
    Bass808Param {
        name,
        lo,
        hi,
        unit,
        stepped: false,
    }
}

/// Every parameter, in index order.
pub const BASS808_PARAMS: [Bass808Param; 13] = [
    param("Decay", 0.05, 60.0, "s"),
    param("Release", 0.005, 20.0, "s"),
    param("Glide", 0.0, 1000.0, "ms"),
    param("Drive", 0.0, 1.0, ""),
    param("Punch", 0.0, 48.0, "st"),
    param("Punch Time", 2.0, 300.0, "ms"),
    param("Tune", -24.0, 24.0, "st"),
    param("Level", -48.0, 12.0, "dB"),
    Bass808Param {
        stepped: true,
        ..param("Legato", 0.0, 1.0, "")
    },
    param("Sine", 0.0, 1.0, ""),
    param("Square", 0.0, 1.0, ""),
    // At the bottom of its range the filter is off.
    param("High Pass", 20.0, 1000.0, "Hz"),
    Bass808Param {
        stepped: true,
        ..param("Sustain", 0.0, 1.0, "")
    },
];

/// How the 808 plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bass808Params {
    /// Seconds the level takes to fall 60 dB.
    pub decay_s: f32,
    /// Seconds a let-go note takes to fall 60 dB.
    pub release_s: f32,
    /// A slide's length, in milliseconds.
    pub glide_ms: f32,
    /// 0 (clean) to 1 (hard tanh drive).
    pub drive: f32,
    /// Semitones the pitch starts above the note.
    pub punch_st: f32,
    /// Milliseconds the punch takes to fall to 5 %.
    pub punch_ms: f32,
    /// Semitones added to every note.
    pub tune_st: f32,
    pub level_db: f32,
    /// Overlapping notes slide without starting again.
    pub legato: bool,
    /// The sine under the voice: 0 (silent) to 1 (as loud as the voice).
    pub sine: f32,
    /// The square under the voice, likewise.
    pub square: f32,
    /// The high-pass filter's cutoff in Hz; off at the bottom of the range (20).
    pub high_pass_hz: f32,
    /// A one-shot holds on a loop of its tail and does not end with its file.
    pub sustain: bool,
}

impl Default for Bass808Params {
    fn default() -> Self {
        Self {
            decay_s: 1.6,
            release_s: 0.08,
            glide_ms: 60.0,
            drive: 0.25,
            punch_st: 12.0,
            punch_ms: 45.0,
            tune_st: 0.0,
            level_db: -6.0,
            legato: true,
            sine: 0.0,
            square: 0.0,
            high_pass_hz: BASS808_PARAMS[BASS808_HIGH_PASS].lo,
            sustain: false,
        }
    }
}

impl Bass808Params {
    /// Parameter `index`'s value.
    pub fn get(&self, index: usize) -> Option<f32> {
        Some(match index {
            BASS808_DECAY => self.decay_s,
            BASS808_RELEASE => self.release_s,
            BASS808_GLIDE => self.glide_ms,
            BASS808_DRIVE => self.drive,
            BASS808_PUNCH => self.punch_st,
            BASS808_PUNCH_TIME => self.punch_ms,
            BASS808_TUNE => self.tune_st,
            BASS808_LEVEL => self.level_db,
            BASS808_LEGATO => f32::from(u8::from(self.legato)),
            BASS808_SINE => self.sine,
            BASS808_SQUARE => self.square,
            BASS808_HIGH_PASS => self.high_pass_hz,
            BASS808_SUSTAIN => f32::from(u8::from(self.sustain)),
            _ => return None,
        })
    }

    /// Sets parameter `index`, kept in its range. Non-finite values are ignored.
    pub fn set(&mut self, index: usize, value: f32) {
        let Some(p) = BASS808_PARAMS.get(index) else {
            return;
        };
        if !value.is_finite() {
            return;
        }
        let v = value.clamp(p.lo, p.hi);
        match index {
            BASS808_DECAY => self.decay_s = v,
            BASS808_RELEASE => self.release_s = v,
            BASS808_GLIDE => self.glide_ms = v,
            BASS808_DRIVE => self.drive = v,
            BASS808_PUNCH => self.punch_st = v,
            BASS808_PUNCH_TIME => self.punch_ms = v,
            BASS808_TUNE => self.tune_st = v,
            BASS808_LEVEL => self.level_db = v,
            BASS808_LEGATO => self.legato = v >= 0.5,
            BASS808_SINE => self.sine = v,
            BASS808_SQUARE => self.square = v,
            BASS808_HIGH_PASS => self.high_pass_hz = v,
            _ => self.sustain = v >= 0.5,
        }
    }

    /// Every value kept in its range.
    pub fn clamped(mut self) -> Self {
        for i in 0..BASS808_PARAMS.len() {
            if let Some(v) = self.get(i) {
                self.set(i, v);
            }
        }
        self
    }
}

/// What the UI makes an 808 from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bass808Spec {
    /// A one-shot to play; empty for the synthesized voice.
    pub sample: String,
    /// The sample's fundamental in Hz (0: measure it when the sample is read).
    pub root_hz: f32,
    pub params: Bass808Params,
}

/// A one-shot made ready to be an 808: mono, at `rate`, leading silence (below -48 dB of the
/// peak) and DC taken off, peak normalised to 1.
pub fn prepare_sample(path: &Path, rate: u32) -> Result<Vec<f32>, String> {
    let stereo = crate::sample_rack::load(path, rate)?;
    let mut mono: Vec<f32> = stereo
        .as_chunks::<2>()
        .0
        .iter()
        .map(|f| 0.5 * (f[0] + f[1]))
        .collect();
    if mono.is_empty() {
        return Err("the file has no sound".into());
    }
    let mean = mono.iter().map(|x| f64::from(*x)).sum::<f64>() / mono.len() as f64;
    for x in &mut mono {
        *x -= mean as f32;
    }
    let peak = mono.iter().fold(0f32, |m, x| m.max(x.abs()));
    if peak <= 0.0 {
        return Err("the file is silent".into());
    }
    let floor = peak * 10f32.powf(-48.0 / 20.0);
    let start = mono
        .iter()
        .position(|x| x.abs() > floor)
        .unwrap_or(0)
        .saturating_sub(8);
    mono.drain(..start);
    for x in &mut mono {
        *x /= peak;
    }
    // The first frames fade in (the 8 kept before the onset), so the start never clicks.
    for (i, x) in mono.iter_mut().take(8).enumerate() {
        *x *= i as f32 / 8.0;
    }
    Ok(mono)
}

/// A Hann-windowed DFT bin's magnitude at `hz` (Goertzel).
fn magnitude_at(seg: &[f32], window: &[f32], hz: f64, rate: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * hz / rate;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0f64, 0f64);
    for (x, h) in seg.iter().zip(window) {
        let s0 = f64::from(*x * *h) + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt()
}

/// The fundamental of an 808 one-shot (mono at `rate`), in Hz: the strongest frequency between
/// 25 and 140 Hz in its sustain (0.25–0.9 s, or from 0.05 s when shorter), found on a 0.5 Hz
/// grid and refined to 0.01 Hz. `None` when the sound is too short to measure.
pub fn measure_root_hz(mono: &[f32], rate: u32) -> Option<f32> {
    let r = rate as usize;
    let (a, b) = (r / 4, mono.len().min(r * 9 / 10));
    let seg = if b > a && b - a >= 4096 {
        &mono[a..b]
    } else {
        mono.get(r / 20..)?
    };
    if seg.len() < 1024 {
        return None;
    }
    let n = seg.len();
    let window: Vec<f32> = (0..n)
        .map(|i| {
            let x = std::f32::consts::TAU * i as f32 / (n - 1) as f32;
            0.5 - 0.5 * x.cos()
        })
        .collect();
    let rate = f64::from(rate);
    let best = |lo: f64, hi: f64, step: f64| {
        let mut f = lo;
        let (mut at, mut top) = (lo, -1f64);
        while f <= hi {
            let m = magnitude_at(seg, &window, f, rate);
            if m > top {
                top = m;
                at = f;
            }
            f += step;
        }
        (at, top)
    };
    let (coarse, top) = best(f64::from(ROOT_LO), f64::from(ROOT_HI), 0.5);
    if top <= 0.0 {
        return None;
    }
    let (fine, _) = best(coarse - 0.5, coarse + 0.5, 0.01);
    Some(fine as f32)
}

/// What keeps a one-shot sounding after its own audio: a loop cut from a steady stretch of its
/// tail. The one-shot plays as it is up to `start`; from there the loop plays, round and round.
///
/// The loop is a whole number of periods long, so it has no beat of its own; its level is
/// evened out (the one-shot's own fall across the stretch is divided away), so it holds at the
/// level the one-shot had at `start` and does not pulse once a turn; and its end is faded
/// into the audio just before `start`, so the turn has no step. `ratio` is the speed that puts
/// the loop exactly on the one-shot's measured fundamental (a whole number of frames is
/// rarely a whole number of periods).
struct SustainLoop {
    start: usize,
    buf: Vec<f32>,
    ratio: f64,
    /// How alike the audio is at the loop's two ends (correlation, at least [`LOOP_LIKENESS`]).
    /// Kept for the tests to report.
    #[cfg_attr(not(test), allow(dead_code))]
    likeness: f64,
}

/// The pitch a one-shot's loop is cut at: the measured one, unless a root set by hand is more
/// than a semitone from it (the measurement found a harmonic, or nothing below 25 Hz).
fn loop_hz(measured: Option<f32>, set: f32) -> f32 {
    let set = if set.is_finite() && set > 0.0 {
        set
    } else {
        0.0
    };
    match measured {
        Some(m) if set > 0.0 && (12.0 * (set / m).log2()).abs() > 1.0 => set,
        Some(m) => m,
        None => set,
    }
}

/// A one-shot read, prepared and analysed (UI thread), as the 808 plays it.
#[derive(Clone)]
struct Prepared {
    mono: Arc<[f32]>,
    measured: Option<f32>,
    /// Its loops, by the pitch they were cut at (`f32::to_bits`); `None` where none fits.
    loops: Vec<(u32, Option<Arc<SustainLoop>>)>,
}

/// The one-shots last used: path, sample rate, the file's size and time, and what was made of
/// it. Most recent last.
type PreparedKey = (String, u32, Option<(u64, std::time::SystemTime)>);
/// A prepared one-shot as the 808 takes it: the sound, its measured pitch and its loop.
type PreparedSample = (Arc<[f32]>, Option<f32>, Option<Arc<SustainLoop>>);
static PREPARED: Mutex<Vec<(PreparedKey, Prepared)>> = Mutex::new(Vec::new());

/// The one-shot at `path` at `rate`, and its loop for the root `root_hz` (0: measured), from
/// the cache or made now (UI thread: reads the file).
fn prepared(path: &str, rate: u32, root_hz: f32) -> Result<PreparedSample, String> {
    let stamp = std::fs::metadata(path)
        .ok()
        .and_then(|m| Some((m.len(), m.modified().ok()?)));
    let key: PreparedKey = (path.to_string(), rate, stamp);
    let mut cache = PREPARED.lock().unwrap_or_else(|e| e.into_inner());
    let i = match cache.iter().position(|(k, _)| *k == key) {
        Some(i) => {
            let hit = cache.remove(i);
            cache.push(hit);
            cache.len() - 1
        }
        None => {
            // Read without the lock held would be kinder to a second thread; there is none.
            let mono = prepare_sample(Path::new(path), rate)?;
            let measured = measure_root_hz(&mono, rate);
            if cache.len() >= PREPARED_KEPT {
                cache.remove(0);
            }
            cache.push((
                key,
                Prepared {
                    mono: mono.into(),
                    measured,
                    loops: Vec::new(),
                },
            ));
            cache.len() - 1
        }
    };
    let entry = &mut cache[i].1;
    let hz = loop_hz(entry.measured, root_hz);
    let lp = match entry.loops.iter().find(|(b, _)| *b == hz.to_bits()) {
        Some((_, lp)) => lp.clone(),
        None => {
            let lp = SustainLoop::new(&entry.mono, rate, hz).map(Arc::new);
            entry.loops.push((hz.to_bits(), lp.clone()));
            lp
        }
    };
    Ok((entry.mono.clone(), entry.measured, lp))
}

/// Whether the 808 `spec` can hold its one-shot with Sustain: a loop fits in it (UI thread;
/// reads and analyses the file the first time). `false` for the synthesized voice.
pub fn can_sustain(spec: &Bass808Spec, rate: u32) -> bool {
    !spec.sample.is_empty()
        && prepared(&spec.sample, rate, spec.root_hz).is_ok_and(|(_, _, lp)| lp.is_some())
}

/// How alike `a` and `b` are, -1 to 1 (their correlation, whatever their levels).
fn likeness(a: &[f32], b: &[f32]) -> f64 {
    let (mut ab, mut aa, mut bb) = (0f64, 0f64, 0f64);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (f64::from(*x), f64::from(*y));
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa > 0.0 && bb > 0.0 {
        ab / (aa * bb).sqrt()
    } else {
        0.0
    }
}

impl SustainLoop {
    /// The loop for the one-shot `s` (mono at `rate`) whose fundamental is `hz`; `None` when
    /// it is too short to hold one. UI thread.
    fn new(s: &[f32], rate: u32, hz: f32) -> Option<Self> {
        if !(hz.is_finite() && hz > 0.0) {
            return None;
        }
        let period = f64::from(rate) / f64::from(hz);
        let p = period.round() as usize;
        if p < 2 {
            return None;
        }
        // The level, period by period.
        let env: Vec<f64> = s
            .chunks_exact(p)
            .map(|c| (c.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / p as f64).sqrt())
            .collect();
        let top = env.iter().fold(0f64, |m, e| m.max(*e));
        if top <= 0.0 {
            return None;
        }
        // The stretch compared at each end of a loop, and the first place a loop may start:
        // after the attack and its falling pitch (a quarter second, less in a short one).
        let w = 2 * p;
        let first = ((rate as usize / 4).min(s.len() * 3 / 10) / p).max(1);
        // An even number of periods: when `hz` is really the 2nd harmonic (a measurement an
        // octave high), the loop is still a whole number of the fundamental's periods, so its
        // ends meet in phase. (Any other mismatch fails the likeness test below.)
        let mut periods = LOOP_PERIODS
            .max((LOOP_SECONDS * f64::from(rate) / period).ceil() as usize)
            .next_multiple_of(2);
        // Shorter loops to try when one does not fit or its ends do not match.
        let shorter = |n: usize| if n > 8 { n - 2 } else { n.saturating_sub(4) };
        while periods >= 2 {
            let n0 = (periods as f64 * period).round() as usize;
            // Starts on whole periods, while the loop and what is compared after it fit and
            // its end is still well above the noise.
            let fits = |j: usize| {
                let l = j * p;
                l + n0 + p / 2 + w + 2 <= s.len()
                    && env
                        .get((l + n0) / p)
                        .is_some_and(|e| *e >= LOOP_FLOOR * top)
            };
            let score = |j: usize| likeness(&s[j * p..j * p + w], &s[j * p + n0..j * p + n0 + w]);
            let best = (first..env.len())
                .take_while(|j| fits(*j))
                .map(score)
                .fold(f64::MIN, f64::max);
            // The earliest start about as steady as the steadiest: the earlier, the louder.
            let Some(j) = (first..env.len())
                .take_while(|j| fits(*j))
                .find(|j| score(*j) >= best - 0.002)
            else {
                periods = shorter(periods);
                continue;
            };
            let start = j * p;
            // The length, to the frame: where the audio a loop later is most like the start,
            // within an eighth of a period of `periods` periods (a wider search could land on
            // a whole number of periods of another pitch, and the loop would play at that).
            let mut n = n0;
            let mut top_like = f64::MIN;
            for len in n0 - p / 8..=n0 + p / 8 {
                let like = likeness(&s[start..start + w], &s[start + len..start + len + w]);
                if like > top_like {
                    top_like = like;
                    n = len;
                }
            }
            if top_like < LOOP_LIKENESS {
                periods = shorter(periods);
                continue;
            }
            let fade = (n / 4).min(start);
            // The one-shot's level at frame `i` (between the periods' middles, a straight line
            // in dB), and the gain that brings frame `i` to the level at `start`.
            let floor = top * 1e-4;
            let ln_level = |i: usize| {
                let x = (i as f64 / p as f64 - 0.5).clamp(0.0, (env.len() - 1) as f64);
                let j = (x as usize).min(env.len() - 1);
                let k = (j + 1).min(env.len() - 1);
                let t = x - j as f64;
                env[j].max(floor).ln() * (1.0 - t) + env[k].max(floor).ln() * t
            };
            let at_start = ln_level(start);
            let even = |i: usize| f64::from(s[i]) * (at_start - ln_level(i)).exp();
            let buf: Vec<f32> = (0..n)
                .map(|i| {
                    let y = even(start + i);
                    let into = i + fade;
                    if into < n {
                        return y as f32;
                    }
                    // The last `fade` frames turn into the audio before `start`.
                    let t = (into - n) as f64 / fade as f64;
                    let wgt = 0.5 - 0.5 * (std::f64::consts::PI * t).cos();
                    (y * (1.0 - wgt) + even(start + i - n) * wgt) as f32
                })
                .collect();
            let ratio = f64::from(hz) * n as f64 / (periods as f64 * f64::from(rate));
            return Some(Self {
                start,
                buf,
                ratio,
                likeness: top_like,
            });
        }
        None
    }
}

/// Catmull-Rom (cubic Hermite) between `y1` and `y2`, `frac` of the way.
fn cubic(y0: f32, y1: f32, y2: f32, y3: f32, frac: f32) -> f32 {
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * frac + c2) * frac + c1) * frac + y1
}

/// What takes the step out of a square wave's edge at phase 0 (PolyBLEP): `t` is the phase,
/// 0 to 1, and `dt` the phase a frame moves.
fn poly_blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// A square wave at phase `t` (0 to 1) moving `dt` a frame, its edges band-limited.
fn square(t: f64, dt: f64) -> f32 {
    let dt = dt.clamp(1e-9, 0.5);
    let naive = if t < 0.5 { 1.0 } else { -1.0 };
    (naive + poly_blep(t, dt) - poly_blep((t + 0.5).fract(), dt)) as f32
}

/// One 12 dB per octave high-pass (a state-variable filter in its trapezoidal form, which
/// stays sound while its cutoff moves).
#[derive(Clone, Copy, Default)]
struct HighPass {
    s1: f64,
    s2: f64,
}

impl HighPass {
    /// `g` is `tan(pi * cutoff / rate)`, `k` is one over the resonance.
    fn tick(&mut self, x: f64, g: f64, k: f64) -> f64 {
        let hp = (x - (k + g) * self.s1 - self.s2) / (1.0 + g * (g + k));
        let bp = g * hp + self.s1;
        self.s1 = g * hp + bp;
        self.s2 = g * bp + (g * bp + self.s2);
        // Silence in leaves the states falling for ever; end that before they are denormal.
        if self.s1.abs() < 1e-30 {
            self.s1 = 0.0;
        }
        if self.s2.abs() < 1e-30 {
            self.s2 = 0.0;
        }
        hp
    }
}

/// What the two halves of a fourth-order Butterworth filter damp by (one over their Q).
const BUTTERWORTH_K: [f64; 2] = [1.0 / 0.541_196_100_146_197, 1.0 / 1.306_562_964_876_376_5];

/// What a voice is played with: the settings and the sound, the same for the sounding voice
/// and the cut one.
struct Playing<'a> {
    p: &'a Bass808Params,
    sample: Option<&'a [f32]>,
    sustain: Option<&'a SustainLoop>,
    root_hz: f32,
    bend_st: f32,
    rate: f32,
    /// The waves' levels now (smoothed).
    sine: f32,
    square: f32,
}

/// One sounding note.
#[derive(Clone, Copy, Default)]
struct Voice {
    active: bool,
    /// The note's pitch (MIDI note number, fractional while sliding), without tune or bend.
    pitch: f32,
    /// Where a slide goes and how far it moves a frame, and frames left.
    glide_to: f32,
    glide_from: f32,
    glide_frames: u32,
    glide_left: u32,
    /// The synth's phase (0..1) or the sample's read position (frames).
    phase: f64,
    /// The waves' phase (0..1).
    wave: f64,
    /// A one-shot that has played to its end (the waves may sound on).
    sample_done: bool,
    /// A one-shot that holds on its loop (set when the voice starts, so Sustain switched
    /// while it sounds does not jump).
    looping: bool,
    /// Frames since the voice started (for the punch and the attack).
    age: u32,
    /// The decay envelope, and the release one once let go.
    env: f32,
    released: bool,
    vel: f32,
    /// A cut voice: its gain, falling by `fade_step` each frame.
    fade: f32,
    fade_step: f32,
}

#[derive(Clone, Copy)]
struct Event {
    offset: u32,
    status: u8,
    d1: u8,
    d2: u8,
}

/// The 808 on the audio thread. Made on the UI thread (it reads its sample there).
pub struct Bass808 {
    rate: f32,
    params: Bass808Params,
    /// Shared with the cache of prepared one-shots, which keeps a reference: dropping the
    /// 808 never frees them.
    sample: Option<Arc<[f32]>>,
    sustain: Option<Arc<SustainLoop>>,
    root_hz: f32,
    /// The waves' levels and the filter's cutoff as they are now, following the settings.
    sine: f32,
    square: f32,
    cutoff_hz: f32,
    /// How far the filter is in the path: 0 (out, the sound untouched) to 1.
    filter_mix: f32,
    /// `tan(pi * cutoff / rate)` and the cutoff it was worked out for.
    filter_g: f64,
    filter_g_for: f32,
    filter: [HighPass; 2],
    voice: Voice,
    /// The voice a new note cut, fading out.
    tail: Voice,
    held: [u8; HELD],
    held_n: usize,
    bend_st: f32,
    events: Vec<Event>,
}

impl Bass808 {
    /// Reads the sample, if there is one (UI thread). An unreadable sample leaves the
    /// synthesized voice playing, and says why.
    pub fn new(spec: &Bass808Spec, sample_rate: u32) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let mut sample = None;
        let mut sustain = None;
        let mut root_hz = spec.root_hz;
        if !spec.sample.is_empty() {
            match prepared(&spec.sample, sample_rate, spec.root_hz) {
                Ok((s, measured, lp)) => {
                    if !(root_hz.is_finite() && root_hz > 0.0) {
                        root_hz = measured.unwrap_or(0.0);
                    }
                    if root_hz > 0.0 {
                        sustain = lp;
                        sample = Some(s);
                    } else {
                        errors.push(format!("{}: no pitch found", spec.sample));
                    }
                }
                Err(e) => errors.push(format!("{}: {e}", spec.sample)),
            }
        }
        let params = spec.params.clamped();
        (
            Self {
                rate: sample_rate.max(1) as f32,
                params,
                sample,
                sustain,
                root_hz,
                sine: params.sine,
                square: params.square,
                cutoff_hz: params.high_pass_hz,
                filter_mix: if params.high_pass_hz > HIGH_PASS_OFF {
                    1.0
                } else {
                    0.0
                },
                filter_g: 0.0,
                filter_g_for: 0.0,
                filter: [HighPass::default(); 2],
                voice: Voice::default(),
                tail: Voice::default(),
                held: [0; HELD],
                held_n: 0,
                bend_st: 0.0,
                events: Vec::with_capacity(MAX_EVENTS),
            },
            errors,
        )
    }

    fn glide_frames(&self) -> u32 {
        (self.params.glide_ms * 0.001 * self.rate) as u32
    }

    /// Slides (or jumps, with no glide) the sounding voice to `pitch` without starting it again.
    fn slide_to(&mut self, pitch: f32) {
        let n = self.glide_frames();
        let v = &mut self.voice;
        if n == 0 {
            v.pitch = pitch;
            v.glide_left = 0;
        } else {
            v.glide_to = pitch;
            v.glide_from = v.pitch;
            v.glide_frames = n;
            v.glide_left = n;
        }
    }

    /// A new voice for `note`; the sounding one is cut. `from` is the pitch it slides from.
    fn start(&mut self, note: u8, vel: u8, from: Option<f32>) {
        if self.voice.active {
            self.tail = self.voice;
            self.tail.fade = 1.0;
            self.tail.fade_step = 1.0 / (CUT_SECONDS * self.rate).max(1.0);
        }
        let target = f32::from(note);
        self.voice = Voice {
            active: true,
            pitch: from.unwrap_or(target),
            env: 1.0,
            vel: f32::from(vel.min(127)) / 127.0,
            fade: 1.0,
            looping: self.params.sustain && self.sustain.is_some(),
            ..Voice::default()
        };
        if from.is_some() {
            self.slide_to(target);
        }
    }

    fn hold(&mut self, note: u8) {
        self.release_key(note);
        if self.held_n == HELD {
            self.held.copy_within(1.., 0);
            self.held_n -= 1;
        }
        self.held[self.held_n] = note;
        self.held_n += 1;
    }

    /// Takes `note` off the held stack; whether it was there.
    fn release_key(&mut self, note: u8) -> bool {
        let Some(i) = self.held[..self.held_n].iter().position(|n| *n == note) else {
            return false;
        };
        self.held.copy_within(i + 1..self.held_n, i);
        self.held_n -= 1;
        true
    }

    fn sounding_note(&self) -> f32 {
        if self.voice.glide_left > 0 {
            self.voice.glide_to
        } else {
            self.voice.pitch
        }
    }

    fn note_on(&mut self, note: u8, vel: u8) {
        let busy = self.voice.active && !self.voice.released;
        let overlapping = self.held_n > 0 && busy;
        self.hold(note);
        if self.params.legato && overlapping {
            self.slide_to(f32::from(note));
            return;
        }
        let from = (!self.params.legato && self.voice.active && self.params.glide_ms > 0.0)
            .then_some(self.voice.pitch);
        self.start(note, vel, from);
    }

    fn note_off(&mut self, note: u8) {
        if !self.release_key(note) {
            return;
        }
        if !self.voice.active || self.voice.released {
            return;
        }
        if let Some(&back) = self.held[..self.held_n].last() {
            // The sounding note let go while others are held: back to the last of them.
            if (self.sounding_note() - f32::from(note)).abs() < 1e-3 {
                self.slide_to(f32::from(back));
            }
            return;
        }
        self.voice.released = true;
    }

    /// All Notes Off: every note let go, sounding on in its release.
    fn all_off(&mut self) {
        self.held_n = 0;
        self.voice.released = true;
    }

    /// All Sound Off: silent now, with the 4 ms fade a cut note has (no click).
    fn all_sound_off(&mut self) {
        self.held_n = 0;
        let step = 1.0 / (CUT_SECONDS * self.rate).max(1.0);
        for v in [&mut self.voice, &mut self.tail] {
            if v.active {
                v.released = true;
                v.fade_step = v.fade_step.max(step);
            }
        }
    }

    fn handle(&mut self, e: Event) {
        match e.status & 0xf0 {
            0x90 if e.d2 > 0 => self.note_on(e.d1, e.d2),
            0x80 | 0x90 => self.note_off(e.d1),
            0xe0 => {
                let v = ((i32::from(e.d2) << 7) | i32::from(e.d1)) - 8192;
                self.bend_st = v as f32 / 8192.0 * BEND_SEMITONES;
            }
            0xb0 if e.d1 == 120 => self.all_sound_off(),
            0xb0 if e.d1 == 123 => self.all_off(),
            _ => {}
        }
    }

    /// One frame of `v` (dry, before drive and level); advances it.
    fn tick(v: &mut Voice, c: &Playing) -> f32 {
        if !v.active {
            return 0.0;
        }
        let (p, rate) = (c.p, c.rate);
        if v.glide_left > 0 {
            v.glide_left -= 1;
            // From how far it has gone, not by adding a step a frame: tiny steps (a long slide
            // over a small interval) would be rounded away and the end would jump.
            v.pitch = if v.glide_left == 0 {
                v.glide_to
            } else {
                let done = 1.0 - f64::from(v.glide_left) / f64::from(v.glide_frames);
                (f64::from(v.glide_from) + f64::from(v.glide_to - v.glide_from) * done) as f32
            };
        }
        let t = v.age as f32 / rate;
        // The punch falls to 5 % (e^-3) over its time.
        let punch = p.punch_st * (-3.0 * t / (p.punch_ms * 0.001).max(1e-4)).exp();
        let semis = v.pitch + p.tune_st + c.bend_st + punch;
        let hz = 440.0 * ((semis - 69.0) / 12.0).exp2();
        let waves = c.sine > 0.0 || c.square > 0.0;
        let attack = (t / ATTACK_SECONDS).min(1.0);
        let mut y = match c.sample {
            Some(_) if v.sample_done => {
                if !waves {
                    v.active = false;
                    return 0.0;
                }
                0.0
            }
            Some(s) => {
                let step = f64::from(hz / c.root_hz.max(1.0));
                match c.sustain {
                    Some(lp) if v.looping && v.phase >= lp.start as f64 => {
                        let n = lp.buf.len();
                        let at = v.phase - lp.start as f64;
                        let i = at as usize;
                        let frac = (at - i as f64) as f32;
                        let tap = |k: usize| lp.buf.get(k % n).copied().unwrap_or(0.0);
                        let y = cubic(tap(i + n - 1), tap(i), tap(i + 1), tap(i + 2), frac);
                        v.phase = lp.start as f64 + (at + step * lp.ratio) % n as f64;
                        y
                    }
                    _ => {
                        let i = v.phase as usize;
                        if i + 2 >= s.len() {
                            // The one-shot is over; the note is too unless a wave sounds on.
                            v.sample_done = true;
                            if !waves {
                                v.active = false;
                                return 0.0;
                            }
                            0.0
                        } else {
                            let frac = (v.phase - i as f64) as f32;
                            let at = |k: usize| s.get(k).copied().unwrap_or(0.0);
                            let (y0, y1, y2, y3) =
                                (at(i.wrapping_sub(1)), at(i), at(i + 1), at(i + 2));
                            v.phase += step;
                            cubic(y0, y1, y2, y3, frac)
                        }
                    }
                }
            }
            None => {
                let y = (std::f64::consts::TAU * v.phase).sin() as f32;
                v.phase = (v.phase + f64::from(hz / rate)).fract();
                y * attack
            }
        };
        // The waves, at the voice's pitch. Their phase runs whether or not they sound, so a
        // level brought up in the middle of a note comes in where the wave would be.
        let dt = f64::from(hz / rate);
        if waves {
            let mut w = 0.0;
            if c.sine > 0.0 {
                w += c.sine * (std::f64::consts::TAU * v.wave).sin() as f32;
            }
            if c.square > 0.0 {
                w += c.square * square(v.wave, dt);
            }
            y += w * attack;
        }
        v.wave = (v.wave + dt).fract();
        let fall = |seconds: f32| (-6.907_755 / (seconds.max(1e-3) * rate)).exp();
        v.env *= fall(p.decay_s);
        if v.released {
            v.env *= fall(p.release_s);
        }
        v.age = v.age.saturating_add(1);
        let out = y * v.env * v.vel * v.fade;
        if v.fade_step > 0.0 {
            v.fade -= v.fade_step;
            if v.fade <= 0.0 {
                v.active = false;
            }
        }
        if v.env < SILENT {
            v.active = false;
        }
        out
    }

    /// Moves the waves' levels and the filter one frame towards the settings.
    fn smooth(&mut self) {
        let a = 1.0 / (SMOOTH_SECONDS * self.rate).max(1.0);
        let follow = |now: &mut f32, to: f32, near: f32| {
            *now += (to - *now) * a;
            if (*now - to).abs() < near {
                *now = to;
            }
        };
        follow(&mut self.sine, self.params.sine, 1e-5);
        follow(&mut self.square, self.params.square, 1e-5);
        follow(&mut self.cutoff_hz, self.params.high_pass_hz, 1e-3);
        let to = if self.params.high_pass_hz > HIGH_PASS_OFF {
            1.0
        } else {
            0.0
        };
        self.filter_mix = (self.filter_mix + (to - self.filter_mix).clamp(-a, a)).clamp(0.0, 1.0);
    }

    /// The high-pass filter on `x`. Out of the path (and `x` untouched) while it is off; it
    /// comes in and goes out as a fade between the two, since they differ in phase.
    fn high_pass(&mut self, x: f32) -> f32 {
        if self.filter_mix <= 0.0 {
            self.filter = [HighPass::default(); 2];
            return x;
        }
        if self.cutoff_hz != self.filter_g_for {
            self.filter_g_for = self.cutoff_hz;
            let hz = f64::from(self.cutoff_hz).min(0.45 * f64::from(self.rate));
            self.filter_g = (std::f64::consts::PI * hz / f64::from(self.rate)).tan();
        }
        let mut y = f64::from(x);
        for (f, k) in self.filter.iter_mut().zip(BUTTERWORTH_K) {
            y = f.tick(y, self.filter_g, k);
        }
        let y = y as f32;
        if self.filter_mix >= 1.0 {
            y
        } else {
            // An S-shaped fade: no corner where it starts or ends.
            let m = self.filter_mix;
            x + (y - x) * (m * m * (3.0 - 2.0 * m))
        }
    }

    fn frame(&mut self, gain: f32) -> f32 {
        self.smooth();
        let c = Playing {
            p: &self.params,
            sample: self.sample.as_deref(),
            sustain: self.sustain.as_deref(),
            root_hz: self.root_hz,
            bend_st: self.bend_st,
            rate: self.rate,
            sine: self.sine,
            square: self.square,
        };
        let dry = Self::tick(&mut self.voice, &c) + Self::tick(&mut self.tail, &c);
        let k = self.params.drive * 10.0;
        let wet = if k > 0.01 {
            (k * dry).tanh() / k.tanh()
        } else {
            dry
        };
        self.high_pass(wet) * gain
    }
}

impl BlockProcessor for Bass808 {
    fn set_parameter(&mut self, index: usize, value: f32) {
        self.params.set(index, value);
    }

    fn midi(&mut self, status: u8, data1: u8, data2: u8, offset: u32) {
        if self.events.len() < self.events.capacity() {
            self.events.push(Event {
                offset,
                status,
                d1: data1,
                d2: data2,
            });
        }
    }

    fn process(&mut self, _input: (&[f32], &[f32]), out: (&mut [f32], &mut [f32])) {
        let frames = out.0.len().min(out.1.len());
        // By frame, keeping the order of events at one frame (a note off before the next note
        // on). Insertion sort: no allocation, and they nearly always come in order already.
        for i in 1..self.events.len() {
            let mut j = i;
            while j > 0 && self.events[j - 1].offset > self.events[j].offset {
                self.events.swap(j - 1, j);
                j -= 1;
            }
        }
        let gain = 10f32.powf(self.params.level_db / 20.0);
        let mut next = 0;
        for i in 0..frames {
            while next < self.events.len() && self.events[next].offset as usize <= i {
                let e = self.events[next];
                self.handle(e);
                next += 1;
            }
            let y = self.frame(gain);
            out.0[i] = y;
            out.1[i] = y;
        }
        while next < self.events.len() {
            let e = self.events[next];
            self.handle(e);
            next += 1;
        }
        self.events.clear();
    }

    fn latency_seconds(&self) -> f64 {
        0.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn synth(params: Bass808Params) -> Bass808 {
        let spec = Bass808Spec {
            params,
            ..Bass808Spec::default()
        };
        let (b, errors) = Bass808::new(&spec, SR);
        assert!(errors.is_empty());
        b
    }

    fn run(b: &mut Bass808, frames: usize) -> Vec<f32> {
        let (mut l, mut r) = (vec![0f32; frames], vec![0f32; frames]);
        assert_no_alloc::assert_no_alloc(|| b.process((&[], &[]), (&mut l, &mut r)));
        assert_eq!(l, r);
        assert!(l.iter().all(|x| x.is_finite()));
        l
    }

    /// The frequency of `x` from its rising zero crossings.
    fn zero_cross_hz(x: &[f32]) -> f32 {
        let ups: Vec<f32> = x
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
            .map(|(i, w)| i as f32 + w[0] / (w[0] - w[1]))
            .collect();
        let periods = (ups.len() - 1) as f32;
        SR as f32 * periods / (ups[ups.len() - 1] - ups[0])
    }

    fn clean() -> Bass808Params {
        Bass808Params {
            drive: 0.0,
            punch_st: 0.0,
            glide_ms: 0.0,
            decay_s: 8.0,
            level_db: 0.0,
            ..Bass808Params::default()
        }
    }

    #[test]
    fn the_note_sets_the_pitch() {
        for (note, hz) in [(28u8, 41.2034f32), (36, 65.4064), (43, 97.9989)] {
            let mut b = synth(clean());
            b.midi(0x90, note, 127, 0);
            let y = run(&mut b, SR as usize);
            let got = zero_cross_hz(&y[SR as usize / 10..]);
            assert!((got - hz).abs() < 0.05, "note {note}: {got} Hz, want {hz}");
        }
        let mut b = synth(Bass808Params {
            tune_st: 12.0,
            ..clean()
        });
        b.midi(0x90, 28, 127, 0);
        let y = run(&mut b, SR as usize);
        assert!((zero_cross_hz(&y[4800..]) - 82.407).abs() < 0.1);
    }

    #[test]
    fn punch_starts_high_and_falls_to_the_note() {
        let mut b = synth(Bass808Params {
            punch_st: 24.0,
            punch_ms: 50.0,
            ..clean()
        });
        b.midi(0x90, 36, 127, 0);
        let y = run(&mut b, SR as usize / 2);
        let early = zero_cross_hz(&y[..480]);
        let late = zero_cross_hz(&y[SR as usize / 5..]);
        assert!(early > 2.0 * 65.4, "{early}");
        assert!((late - 65.406).abs() < 0.1, "{late}");
    }

    #[test]
    fn a_new_note_cuts_the_last_and_overlaps_slide_with_legato() {
        // Legato: an overlapping note slides, the envelope carries on.
        let mut b = synth(Bass808Params {
            glide_ms: 100.0,
            ..clean()
        });
        b.midi(0x90, 36, 127, 0);
        run(&mut b, 4800);
        let env = b.voice.env;
        b.midi(0x90, 48, 127, 0);
        run(&mut b, 2400);
        assert!(
            b.voice.pitch > 36.0 && b.voice.pitch < 48.0,
            "{}",
            b.voice.pitch
        );
        assert!(b.voice.env < env, "not started again");
        run(&mut b, 4800);
        assert_eq!(b.voice.pitch, 48.0);
        let y = run(&mut b, SR as usize / 2);
        assert!((zero_cross_hz(&y) - 130.81).abs() < 0.1);
        // Letting go of the top note slides back to the one still held.
        b.midi(0x80, 48, 0, 0);
        run(&mut b, 9600);
        assert_eq!(b.voice.pitch, 36.0);
        assert!(!b.voice.released);
        b.midi(0x80, 36, 0, 0);
        run(&mut b, 1);
        assert!(b.voice.released);

        // Without legato a new note starts again (choking the old one), sliding from the last.
        let mut b = synth(Bass808Params {
            legato: false,
            glide_ms: 100.0,
            ..clean()
        });
        b.midi(0x90, 36, 127, 0);
        run(&mut b, 4800);
        b.midi(0x90, 48, 127, 0);
        run(&mut b, 2400);
        assert!(b.voice.age == 2400 && b.voice.pitch > 36.0 && b.voice.pitch < 48.0);
        assert!(!b.tail.active, "the cut voice has faded");
    }

    #[test]
    fn notes_that_do_not_overlap_start_again() {
        let mut b = synth(Bass808Params {
            glide_ms: 100.0,
            ..clean()
        });
        b.midi(0x90, 36, 127, 0);
        b.midi(0x80, 36, 0, 100);
        run(&mut b, 4800);
        b.midi(0x90, 48, 127, 0);
        run(&mut b, 10);
        assert_eq!(b.voice.pitch, 48.0, "no slide between separate notes");
        assert_eq!(b.voice.age, 10);
    }

    #[test]
    fn decay_and_release_shorten_the_note() {
        let level_after = |p: Bass808Params, off: bool| {
            let mut b = synth(p);
            b.midi(0x90, 36, 127, 0);
            if off {
                b.midi(0x80, 36, 0, 0);
            }
            let y = run(&mut b, SR as usize);
            y[SR as usize / 2..]
                .iter()
                .fold(0f32, |m, x| m.max(x.abs()))
        };
        let long = level_after(
            Bass808Params {
                decay_s: 4.0,
                ..clean()
            },
            false,
        );
        let short = level_after(
            Bass808Params {
                decay_s: 0.2,
                ..clean()
            },
            false,
        );
        // 4 s to -60 dB is -7.5 dB at half a second.
        assert!(long > 0.35 && short < 0.01, "{long} {short}");
        let released = level_after(
            Bass808Params {
                decay_s: 4.0,
                release_s: 0.05,
                ..clean()
            },
            true,
        );
        assert!(released < 1e-4, "{released}");
    }

    #[test]
    fn drive_saturates_and_keeps_full_scale() {
        let mut b = synth(Bass808Params {
            drive: 1.0,
            ..clean()
        });
        b.midi(0x90, 36, 127, 0);
        let y = run(&mut b, SR as usize / 2);
        let peak = y.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!(peak <= 1.0 + 1e-4 && peak > 0.95, "{peak}");
        // A driven sine is squarer: its mean level is well above a sine's 2/pi.
        let mean = y[4800..].iter().map(|x| x.abs()).sum::<f32>() / (y.len() - 4800) as f32;
        assert!(mean > 0.75, "{mean}");
    }

    #[test]
    fn a_sample_plays_at_the_note_from_its_measured_root() {
        // A 41.2 Hz "808" (E1) with a falling envelope, as a one-shot would be.
        let root = 41.2f32;
        let s: Vec<f32> = (0..SR as usize * 2)
            .map(|i| {
                let t = i as f32 / SR as f32;
                (std::f32::consts::TAU * root * t).sin() * (-t * 1.5).exp()
            })
            .collect();
        let measured = measure_root_hz(&s, SR).unwrap();
        assert!((measured - root).abs() < 0.05, "{measured}");
        let (mut b, _) = Bass808::new(&Bass808Spec::default(), SR);
        b.params = clean();
        b.sample = Some(s.into());
        b.root_hz = measured;
        b.midi(0x90, 33, 127, 0); // A1, 55 Hz
        let y = run(&mut b, SR as usize);
        let got = zero_cross_hz(&y[4800..]);
        assert!((got - 55.0).abs() < 0.1, "{got}");
        // It ends with the sample: two octaves up (E3), 2 s of it last half a second.
        b.midi(0x80, 33, 0, 0);
        b.midi(0x90, 52, 127, 1);
        run(&mut b, SR as usize);
        assert!(!b.voice.active);
    }

    #[test]
    fn parameters_stay_in_range_and_bend_moves_the_pitch() {
        let mut b = synth(clean());
        b.set_parameter(BASS808_DECAY, 100.0);
        assert_eq!(b.params.decay_s, 60.0);
        b.set_parameter(BASS808_LEVEL, f32::NAN);
        assert_eq!(b.params.level_db, 0.0);
        b.set_parameter(BASS808_LEGATO, 0.0);
        assert!(!b.params.legato);
        b.set_parameter(99, 1.0);
        b.midi(0xe0, 0x7f, 0x7f, 0); // full bend up: +2 semitones
        b.midi(0x90, 36, 127, 0);
        let y = run(&mut b, SR as usize / 2);
        assert!((zero_cross_hz(&y[4800..]) - 73.416).abs() < 0.1);
    }

    #[test]
    fn a_missing_sample_says_so_and_the_synth_plays() {
        let spec = Bass808Spec {
            sample: "/no/such/808.wav".into(),
            ..Bass808Spec::default()
        };
        let (mut b, errors) = Bass808::new(&spec, SR);
        assert_eq!(errors.len(), 1);
        b.midi(0x90, 36, 127, 0);
        assert!(run(&mut b, 4800).iter().any(|x| x.abs() > 0.1));
    }

    /// A fixed performance (notes, slides, a cut note, bend, release), as one number: FNV-1a
    /// over every output sample's bits.
    fn performance_hash(b: &mut Bass808) -> u64 {
        let mut out = Vec::new();
        b.midi(0x90, 36, 127, 0);
        out.extend(run(b, 9600));
        b.midi(0x90, 43, 100, 100);
        out.extend(run(b, 9600));
        b.midi(0x80, 43, 0, 0);
        b.midi(0xe0, 0x00, 0x50, 300);
        out.extend(run(b, 4800));
        b.midi(0x80, 36, 0, 0);
        b.midi(0x90, 31, 90, 2000);
        out.extend(run(b, 24_000));
        b.set_parameter(BASS808_LEGATO, 0.0);
        b.midi(0x90, 40, 127, 7);
        out.extend(run(b, 12_000));
        b.midi(0x80, 40, 0, 0);
        b.midi(0x80, 31, 0, 0);
        out.extend(run(b, 48_000));
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for x in out {
            for byte in x.to_bits().to_le_bytes() {
                h = (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
            }
        }
        h
    }

    fn one_shot() -> Vec<f32> {
        (0..SR as usize * 3 / 2)
            .map(|i| {
                let t = i as f32 / SR as f32;
                let f = 41.2 * (1.0 + (-t * 40.0).exp());
                ((std::f32::consts::TAU * f * t).sin()
                    + 0.2 * (std::f32::consts::TAU * 3.0 * f * t).sin())
                    * (-t * 2.0).exp()
            })
            .collect()
    }

    /// With the waves at 0, the filter off and Sustain off, the 808 plays what it played
    /// before it had them, bit for bit. The numbers are that performance's hashes taken from
    /// the code before the change (on an Apple-silicon Mac: `exp`, `sin` and `tanh` differ in
    /// their last bit between systems, so the check runs there only). Glide is 0: a slide is
    /// now worked out from how far it has gone, which moves its last bits.
    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn with_the_new_settings_off_it_plays_bit_for_bit_what_it_did() {
        let p = Bass808Params {
            glide_ms: 0.0,
            ..Bass808Params::default()
        };
        let mut b = synth(p);
        assert_eq!(performance_hash(&mut b), 0x981b_0e9b_b5f3_e574);
        let mut b = synth(p);
        b.sample = Some(one_shot().into());
        b.root_hz = 41.2;
        assert_eq!(performance_hash(&mut b), 0x8a26_78f8_ff77_8843);
        // A loop being there changes nothing while Sustain is off.
        let mut b = sampled(p, one_shot());
        b.root_hz = 41.2;
        assert!(b.sustain.is_some());
        assert_eq!(performance_hash(&mut b), 0x8a26_78f8_ff77_8843);
    }

    /// An 808 playing the one-shot `s`, as `Bass808::new` makes it from a file.
    fn sampled(params: Bass808Params, s: Vec<f32>) -> Bass808 {
        let mut b = synth(params);
        b.root_hz = measure_root_hz(&s, SR).unwrap();
        b.sustain = SustainLoop::new(&s, SR, b.root_hz).map(Arc::new);
        b.sample = Some(s.into());
        b
    }

    /// A click of a one-shot: over in 20 ms, so what sounds after it is the waves alone.
    fn click() -> Vec<f32> {
        (0..960)
            .map(|i| (i as f32 * 0.05).sin() * (1.0 - i as f32 / 960.0))
            .collect()
    }

    fn waves_only(sine: f32, square: f32) -> Bass808 {
        let mut b = synth(Bass808Params {
            sine,
            square,
            decay_s: 60.0,
            ..clean()
        });
        b.sample = Some(click().into());
        b.root_hz = 55.0;
        b
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0f32, |m, v| m.max(v.abs()))
    }

    /// The largest step between frames, and the largest change of step.
    fn roughness(x: &[f32]) -> (f32, f32) {
        let d1 = x.windows(2).fold(0f32, |m, w| m.max((w[1] - w[0]).abs()));
        let d2 = x
            .windows(3)
            .fold(0f32, |m, w| m.max((w[2] - 2.0 * w[1] + w[0]).abs()));
        (d1, d2)
    }

    #[test]
    fn the_sine_and_the_square_sound_at_the_note() {
        let hz_of = |n: u8| 440.0 * ((f32::from(n) - 69.0) / 12.0).exp2();
        for note in [24u8, 28, 36, 43, 57, 69] {
            for (sine, square) in [(1.0, 0.0), (0.0, 1.0)] {
                let mut b = waves_only(sine, square);
                b.midi(0x90, note, 127, 0);
                // The decay (60 dB in 60 s) taken off, so the shape can be read.
                let y: Vec<f32> = run(&mut b, 2 * SR as usize)
                    .iter()
                    .enumerate()
                    .map(|(i, x)| x / (-6.907_755 * i as f32 / SR as f32 / 60.0).exp())
                    .collect();
                let y = &y[SR as usize / 2..];
                let (got, want) = (zero_cross_hz(y), hz_of(note));
                println!("note {note} sine {sine} square {square}: {got:.4} Hz, want {want:.4}");
                assert!(
                    (got - want).abs() < want * 2e-4,
                    "note {note}: {got}, {want}"
                );
                // A sine's mean level is 2/pi of its peak, a square's nearly all of it.
                let shape = y.iter().map(|x| x.abs()).sum::<f32>() / y.len() as f32 / peak(y);
                if square > 0.0 {
                    assert!(shape > 0.9, "{shape}");
                } else {
                    assert!(
                        (shape - std::f32::consts::FRAC_2_PI).abs() < 0.02,
                        "{shape}"
                    );
                }
            }
        }
        // Tune and bend move the waves as they move the voice.
        let mut b = waves_only(1.0, 0.0);
        b.set_parameter(BASS808_TUNE, 12.0);
        b.midi(0xe0, 0x7f, 0x7f, 0);
        b.midi(0x90, 28, 127, 0);
        let y = run(&mut b, SR as usize);
        let got = zero_cross_hz(&y[SR as usize / 2..]);
        assert!((got - hz_of(42)).abs() < 0.05, "{got}");
    }

    #[test]
    fn a_wave_is_as_loud_as_its_level_says_and_silent_at_zero() {
        let level = |sine: f32, square: f32| {
            let mut b = waves_only(sine, square);
            b.midi(0x90, 36, 127, 0);
            let y = run(&mut b, SR as usize / 2);
            peak(&y[4800..9600])
        };
        // After a tenth of a second a 60 s decay has taken 0.1 dB.
        let full = level(1.0, 0.0);
        assert!((full - 0.988).abs() < 0.01, "{full}");
        assert!((level(0.5, 0.0) - 0.5 * full).abs() < 0.005);
        assert!((level(0.0, 0.5) - 0.5 * full).abs() < 0.03);
        // With both at zero the note ends where the one-shot ends, as it always did.
        let mut b = waves_only(0.0, 0.0);
        b.midi(0x90, 36, 127, 0);
        let y = run(&mut b, 4800);
        assert!(!b.voice.active);
        assert_eq!(peak(&y[2400..]), 0.0);
    }

    #[test]
    fn the_waves_share_the_synthesized_voices_phase_and_slide_with_it() {
        // The sine at full level on the synthesized voice is the voice twice over, punch and all.
        let play = |sine: f32| {
            let mut b = synth(Bass808Params {
                sine,
                drive: 0.0,
                level_db: 0.0,
                glide_ms: 80.0,
                ..Bass808Params::default()
            });
            b.midi(0x90, 36, 127, 0);
            let mut y = run(&mut b, 9600);
            b.midi(0x90, 43, 127, 0);
            y.extend(run(&mut b, 9600));
            assert_eq!(
                b.voice.age, 19_200,
                "an overlapping note slides: nothing starts again"
            );
            y
        };
        let (one, two) = (play(0.0), play(1.0));
        for (a, b) in one.iter().zip(&two) {
            assert!((2.0 * a - b).abs() < 1e-5, "{a} {b}");
        }
        // A square under a slide has no step in it but its own edges: the same as on a held note.
        let mut b = waves_only(1.0, 0.0);
        b.set_parameter(BASS808_GLIDE, 100.0);
        b.midi(0x90, 36, 127, 0);
        run(&mut b, 9600);
        b.midi(0x90, 48, 127, 0);
        let y = run(&mut b, 9600);
        let (d1, _) = roughness(&y);
        // The steepest a full-scale sine an octave up (130.8 Hz) gets.
        assert!(
            d1 < 1.05 * std::f32::consts::TAU * 130.82 / SR as f32,
            "{d1}"
        );
        assert!((zero_cross_hz(&run(&mut b, SR as usize / 2)) - 130.81).abs() < 0.1);
    }

    #[test]
    fn a_new_note_starts_the_waves_again_without_a_click() {
        let mut b = waves_only(1.0, 0.0);
        b.set_parameter(BASS808_LEGATO, 0.0);
        b.midi(0x90, 36, 127, 0);
        run(&mut b, 10_000);
        b.midi(0x90, 36, 127, 0);
        let y = run(&mut b, 4800);
        assert_eq!(b.voice.age, 4800);
        // The cut note fades in 4 ms: its full-scale sine can fall by 1/192 a frame, on top of
        // the click one-shot's own steps and the new wave's slope.
        let (d1, _) = roughness(&y);
        assert!(d1 < 0.08, "{d1}");
        assert!(
            y[0].abs() > 0.1,
            "the old note is still there on the first frame"
        );
    }

    #[test]
    fn the_square_does_not_alias() {
        // C6, 1046.5 Hz: the 45th harmonic (47092.6 Hz) folds down to 907.4 Hz at 48 kHz.
        let hz = 1_046.502_3_f64;
        let alias = f64::from(SR) - 45.0 * hz;
        let mut b = waves_only(0.0, 1.0);
        b.midi(0x90, 84, 127, 0);
        let y = run(&mut b, 2 * SR as usize);
        let y = &y[SR as usize..];
        let mut phase = 0f64;
        let naive: Vec<f32> = (0..y.len())
            .map(|_| {
                let v = if phase < 0.5 { 1.0 } else { -1.0 };
                phase = (phase + hz / f64::from(SR)).fract();
                v
            })
            .collect();
        let window: Vec<f32> = (0..y.len())
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (y.len() - 1) as f32).cos())
            .collect();
        let at = |x: &[f32], f: f64| magnitude_at(x, &window, f, f64::from(SR));
        let (ours, theirs) = (at(y, alias) / at(y, hz), at(&naive, alias) / at(&naive, hz));
        println!(
            "alias at {alias:.1} Hz under the fundamental: {:.1} dB, a plain square's {:.1} dB",
            20.0 * ours.log10(),
            20.0 * theirs.log10()
        );
        assert!(ours < theirs * 0.1, "{ours} {theirs}");
    }

    #[test]
    fn the_high_pass_takes_out_what_is_below_it_and_leaves_what_is_above() {
        let level = |note: u8, cutoff: f32| {
            let mut b = synth(Bass808Params {
                high_pass_hz: cutoff,
                decay_s: 60.0,
                ..clean()
            });
            b.midi(0x90, note, 127, 0);
            let y = run(&mut b, SR as usize);
            peak(&y[SR as usize / 2..])
        };
        let off = BASS808_PARAMS[BASS808_HIGH_PASS].lo;
        // C1 (65.4 Hz) two octaves under the cutoff: 24 dB an octave is 48 dB down.
        let low = level(36, 261.63) / level(36, off);
        // A3 (220 Hz) at the cutoff: 3 dB down. C6 far above 100 Hz: untouched.
        let at = level(57, 220.0) / level(57, off);
        let high = level(84, 100.0) / level(84, off);
        println!(
            "two octaves below {:.2} dB, at the cutoff {:.2} dB, above {:.3} dB",
            20.0 * low.log10(),
            20.0 * at.log10(),
            20.0 * high.log10()
        );
        assert!((20.0 * low.log10() + 48.16).abs() < 0.5, "{low}");
        assert!((20.0 * at.log10() + 3.01).abs() < 0.1, "{at}");
        assert!((20.0 * high.log10()).abs() < 0.01, "{high}");
        // One octave under: 24 dB.
        let octave = level(45, 220.0) / level(45, off);
        assert!((20.0 * octave.log10() + 24.1).abs() < 0.4, "{octave}");
    }

    #[test]
    fn turning_the_filter_or_a_wave_while_a_note_sounds_does_not_click() {
        // The steepest a full-scale 65.4 Hz sine gets, a frame.
        let slope = std::f32::consts::TAU * 65.406 / SR as f32;
        let mut b = synth(Bass808Params {
            decay_s: 60.0,
            ..clean()
        });
        b.midi(0x90, 36, 127, 0);
        run(&mut b, 9600);
        let mut y = Vec::new();
        for hz in [300.0, 20.0, 1000.0, 20.0, 80.0] {
            b.set_parameter(BASS808_HIGH_PASS, hz);
            y.extend(run(&mut b, 4800));
        }
        let (d1, d2) = roughness(&y);
        println!("filter turned: step {d1:.5} (a sine's {slope:.5}), change of step {d2:.6}");
        // A click is a step: none here. (The slope does turn, by as much as a filter whose
        // cutoff is swept in 10 ms turns it.)
        assert!(d1 < 1.3 * slope && d2 < 0.01, "{d1} {d2}");
        // Off again, the filter is out of the path altogether.
        b.set_parameter(BASS808_HIGH_PASS, 20.0);
        run(&mut b, 4800);
        assert_eq!(b.filter_mix, 0.0);
        // A wave brought up and taken away under a held note.
        let mut y = Vec::new();
        for (sine, square) in [(1.0, 0.0), (0.0, 0.0), (0.3, 0.0), (0.0, 0.0)] {
            b.set_parameter(BASS808_SINE, sine);
            b.set_parameter(BASS808_SQUARE, square);
            y.extend(run(&mut b, 9600));
        }
        let (d1, d2) = roughness(&y);
        println!("sine turned: step {d1:.5}, change of step {d2:.6}");
        assert!(d1 < 2.2 * slope && d2 < 0.002, "{d1} {d2}");
        assert_eq!(b.sine, 0.0, "back at zero exactly");
    }

    #[test]
    fn decay_and_release_reach_a_minute_and_twenty_seconds() {
        let mut b = synth(clean());
        b.set_parameter(BASS808_DECAY, 100.0);
        b.set_parameter(BASS808_RELEASE, 100.0);
        assert_eq!((b.params.decay_s, b.params.release_s), (60.0, 20.0));
        // Ten seconds into a held note: a 60 s decay has taken 10 dB, where the old longest
        // (8 s) had taken 75 dB.
        b.midi(0x90, 36, 127, 0);
        run(&mut b, 10 * SR as usize);
        let y = run(&mut b, SR as usize / 10);
        let db = 20.0 * peak(&y).log10();
        println!("ten seconds in: {db:.2} dB");
        assert!((db + 10.0).abs() < 0.1, "{db}");
        assert!((zero_cross_hz(&y) - 65.406).abs() < 0.05);
    }

    #[test]
    fn sustain_holds_a_one_shot_in_tune_long_after_its_file_has_ended() {
        let s = one_shot();
        let lp = SustainLoop::new(&s, SR, measure_root_hz(&s, SR).unwrap()).unwrap();
        println!(
            "loop: from {:.3} s, {:.3} s long, speed {:.6}",
            lp.start as f32 / SR as f32,
            lp.buf.len() as f32 / SR as f32,
            lp.ratio
        );
        assert!(lp.start >= SR as usize / 5 && lp.buf.len() >= SR as usize / 4);
        assert!((lp.ratio - 1.0).abs() < 1e-3, "{}", lp.ratio);
        let params = Bass808Params {
            sustain: true,
            decay_s: 60.0,
            ..clean()
        };
        // A1 (55 Hz) from a 41.2 Hz one-shot 1.5 s long: its own audio is over after 1.12 s.
        let seconds = 12;
        let mut b = sampled(params, s.clone());
        b.midi(0x90, 33, 127, 0);
        let y = run(&mut b, seconds * SR as usize);
        assert!(b.voice.active);
        let sr = SR as usize;
        let late = &y[10 * sr..11 * sr];
        let hz = zero_cross_hz(&y[6 * sr..11 * sr]);
        println!(
            "10 to 11 s in: peak {:.4}, {hz:.4} Hz (want 55)",
            peak(late)
        );
        assert!(peak(late) > 0.1, "{}", peak(late));
        assert!((hz - 55.0).abs() < 0.01, "{hz}");
        // No step anywhere after the attack, the loop's every turn included: no frame moves
        // more than the sound's own slope allows, and the slope never jumps.
        // The loop takes over at `into` and first turns `turn` frames later; the first half of
        // that first turn has neither the takeover nor a turn in it.
        let speed = 55.0 / b.root_hz;
        let into = (lp.start as f32 / speed) as usize;
        let turn = (lp.buf.len() as f32 / speed) as usize;
        assert!(into + turn < sr / 2);
        let (d1, d2) = roughness(&y[into - 4..]);
        let (free1, free2) = roughness(&y[into + 100..into + turn / 2]);
        println!(
            "steps: {d1:.6} and {d2:.7} from the takeover on, {free1:.6} and {free2:.7} inside one turn"
        );
        assert!(
            d1 <= free1 * 1.1 && d2 <= free2 * 1.25,
            "{d1} {free1} {d2} {free2}"
        );
        // And in the loop itself: across its turn the audio moves as it does inside it.
        let n = lp.buf.len();
        let seam = [lp.buf[n - 2], lp.buf[n - 1], lp.buf[0], lp.buf[1]];
        let (seam1, seam2) = roughness(&seam);
        let (in1, in2) = roughness(&lp.buf[..n / 2]);
        assert!(seam1 <= in1 && seam2 <= in2, "{seam1} {in1} {seam2} {in2}");
        // The level is held: with the decay taken off, a period's peak is the same from the
        // second second to the eleventh (no pulse once a loop).
        let period = (SR as f32 / 55.0) as usize;
        let peaks: Vec<f32> = y[2 * sr..11 * sr]
            .chunks_exact(period)
            .enumerate()
            .map(|(k, c)| {
                let t = (2 * sr + k * period) as f32 / SR as f32;
                peak(c) / (-6.907_755 * t / 60.0).exp()
            })
            .collect();
        let (lo, hi) = peaks
            .iter()
            .fold((f32::MAX, 0f32), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        println!("held level: {lo:.4} to {hi:.4}");
        assert!(hi / lo < 1.03, "{lo} {hi}");
        // Without Sustain the same note is over with the file.
        let mut b = sampled(
            Bass808Params {
                sustain: false,
                ..params
            },
            s,
        );
        b.midi(0x90, 33, 127, 0);
        let y = run(&mut b, 2 * sr);
        assert!(!b.voice.active);
        assert_eq!(peak(&y[sr * 3 / 2..]), 0.0);
    }

    #[test]
    fn a_sustained_one_shot_slides_and_lets_go_like_any_note() {
        let mut b = sampled(
            Bass808Params {
                sustain: true,
                decay_s: 60.0,
                glide_ms: 100.0,
                release_s: 0.05,
                ..clean()
            },
            one_shot(),
        );
        let sr = SR as usize;
        b.midi(0x90, 33, 127, 0);
        run(&mut b, 3 * sr);
        // An overlapping note slides the loop up an octave.
        b.midi(0x90, 45, 127, 0);
        let y = run(&mut b, 2 * sr);
        let hz = zero_cross_hz(&y[sr..]);
        assert!((hz - 110.0).abs() < 0.03, "{hz}");
        // Sustain switched off under a sounding note leaves that note on its loop.
        b.set_parameter(BASS808_SUSTAIN, 0.0);
        let (d1, _) = roughness(&run(&mut b, sr));
        assert!(b.voice.active && d1 < 0.02, "{d1}");
        b.midi(0x80, 33, 0, 0);
        b.midi(0x80, 45, 0, 0);
        run(&mut b, sr);
        assert!(!b.voice.active, "let go, it ends in its release time");
    }

    #[test]
    fn a_one_shot_too_short_for_a_loop_plays_as_it_is() {
        assert!(SustainLoop::new(&click(), SR, 55.0).is_none());
        assert!(SustainLoop::new(&one_shot(), SR, 0.0).is_none());
        let mut b = waves_only(0.0, 0.0);
        b.set_parameter(BASS808_SUSTAIN, 1.0);
        b.midi(0x90, 36, 127, 0);
        run(&mut b, 4800);
        assert!(!b.voice.active);
    }

    #[test]
    fn playing_with_every_new_setting_does_not_allocate() {
        // `run` plays inside `assert_no_alloc`, which aborts the test on any allocation.
        let mut b = sampled(
            Bass808Params {
                sustain: true,
                sine: 0.7,
                square: 0.4,
                high_pass_hz: 90.0,
                ..Bass808Params::default()
            },
            one_shot(),
        );
        for block in 0..400u32 {
            if block % 40 == 0 {
                b.midi(0x90, 30 + (block / 40) as u8, 110, 3);
            }
            if block % 40 == 30 {
                b.midi(0x80, 30 + (block / 40) as u8, 0, 100);
            }
            b.set_parameter(BASS808_HIGH_PASS, 20.0 + (block % 50) as f32 * 10.0);
            b.set_parameter(BASS808_SINE, (block % 7) as f32 / 7.0);
            assert!(peak(&run(&mut b, 512)) <= 4.0);
        }
    }

    /// Real 808 one-shots held for eight seconds: every `.wav` and `.aif` in the folder
    /// `PROWAVE_808_DIR`, played at the note nearest its own root with Sustain on.
    #[test]
    #[ignore = "needs a folder of 808 one-shots in PROWAVE_808_DIR"]
    fn real_one_shots_hold_in_tune_without_a_step() {
        let dir = std::env::var("PROWAVE_808_DIR").unwrap();
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "wav" || e == "aif"))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no one-shots in {dir}");
        let sr = SR as usize;
        let (mut looped, mut worst_cents, mut worst_pulse, mut worst_step) = (0, 0f32, 0f32, 0f32);
        for path in &files {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let spec = Bass808Spec {
                sample: path.to_string_lossy().into_owned(),
                root_hz: 0.0,
                params: Bass808Params {
                    sustain: true,
                    decay_s: 60.0,
                    ..clean()
                },
            };
            let (mut b, errors) = Bass808::new(&spec, SR);
            assert!(errors.is_empty(), "{errors:?}");
            let seconds = b.sample.as_ref().unwrap().len() as f32 / SR as f32;
            let Some((start, n, ratio, like)) = b
                .sustain
                .as_ref()
                .map(|l| (l.start, l.buf.len(), l.ratio, l.likeness))
            else {
                println!("{name}: {seconds:.2} s, root {:.2} Hz, NO LOOP", b.root_hz);
                continue;
            };
            looped += 1;
            let note = (69.0 + 12.0 * (b.root_hz / 440.0).log2()).round();
            let want = 440.0 * ((note - 69.0) / 12.0).exp2();
            b.midi(0x90, note as u8, 127, 0);
            let y = run(&mut b, 8 * sr);
            assert!(b.voice.active, "{name}");
            // The pitch from 3 s on (long after the file's own end for most): the strongest
            // frequency within a semitone of the note.
            let late = &y[3 * sr..];
            let window: Vec<f32> = (0..late.len())
                .map(|i| {
                    0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (late.len() - 1) as f32).cos()
                })
                .collect();
            let (mut hz, mut top) = (0f64, 0f64);
            let mut f = f64::from(want) * 0.944;
            while f < f64::from(want) * 1.06 {
                let m = magnitude_at(late, &window, f, f64::from(SR));
                if m > top {
                    (hz, top) = (f, m);
                }
                f += 0.005;
            }
            let cents = 1200.0 * (hz as f32 / want).log2();
            // Steps: from just before the loop takes over to the end, against the inside of
            // its first turn.
            let speed = want / b.root_hz;
            let into = (start as f32 / speed) as usize;
            let turn = (n as f32 / speed) as usize;
            let (d1, d2) = roughness(&y[into.saturating_sub(4)..]);
            let (free1, free2) = roughness(&y[into + 50..into + turn / 2]);
            // The held level: the peak of each loop's length of sound from 3 s on, decay
            // taken off.
            let peaks: Vec<f32> = y[3 * sr..]
                .chunks_exact(turn.max(1) / 4)
                .enumerate()
                .map(|(k, c)| {
                    let t = (3 * sr + k * (turn / 4)) as f32 / SR as f32;
                    peak(c) / (-6.907_755 * t / 60.0).exp()
                })
                .collect();
            let (lo, hi) = peaks
                .iter()
                .fold((f32::MAX, 0f32), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            let pulse = 20.0 * (hi / lo).log10();
            println!(
                "{name}: {seconds:.2} s, root {:.2} Hz, loop from {:.3} s for {:.3} s at speed \
                 {ratio:.5}, likeness {like:.5}; held at {hi:.3}, {cents:+.2} cents, pulse {pulse:.2} dB, steps \
                 {:.2}x and {:.2}x a turn's own",
                b.root_hz,
                start as f32 / SR as f32,
                n as f32 / SR as f32,
                d1 / free1,
                d2 / free2
            );
            worst_cents = worst_cents.max(cents.abs());
            worst_pulse = worst_pulse.max(pulse);
            worst_step = worst_step.max(d2 / free2);
        }
        println!(
            "{looped} of {} looped; worst: {worst_cents:.2} cents, pulse {worst_pulse:.2} dB, \
             change of step {worst_step:.2}x",
            files.len()
        );
        assert!(looped > 0);
        assert!(
            worst_cents < 1.0 && worst_pulse < 1.5 && worst_step < 1.5,
            "{worst_cents} {worst_pulse} {worst_step}"
        );
    }

    /// How much a held note's level swings, in dB: the RMS of each stretch of four periods of
    /// `hz` from `from` to `to` seconds, with a 60 s decay taken off, highest over lowest.
    fn swing_db(y: &[f32], hz: f32, from: usize, to: usize) -> f32 {
        let len = (4.0 * SR as f32 / hz) as usize;
        let rms: Vec<f32> = y[from * SR as usize..to * SR as usize]
            .chunks_exact(len)
            .enumerate()
            .map(|(k, c)| {
                let t = (from * SR as usize + k * len) as f32 / SR as f32;
                (c.iter().map(|x| x * x).sum::<f32>() / len as f32).sqrt()
                    / (-6.907_755 * t / 60.0).exp()
            })
            .collect();
        let (lo, hi) = rms
            .iter()
            .fold((f32::MAX, 0f32), |(lo, hi), r| (lo.min(*r), hi.max(*r)));
        20.0 * (hi / lo).log10()
    }

    /// A one-shot at `hz` whose second harmonic is `second` times as loud as its fundamental.
    fn one_shot_at(hz: f32, second: f32, seconds: f32) -> Vec<f32> {
        (0..(SR as f32 * seconds) as usize)
            .map(|i| {
                let t = i as f32 / SR as f32;
                let ph = std::f32::consts::TAU * hz * t;
                (ph.sin() + second * (2.0 * ph).sin()) * (-t * 1.2).exp() * (t * 2000.0).min(1.0)
            })
            .collect()
    }

    /// Holds note `note` of `spec`'s one-shot for eight seconds; the sound and the note's Hz.
    fn hold(spec: &Bass808Spec, note: u8) -> (Vec<f32>, f32, Bass808) {
        let (mut b, errors) = Bass808::new(spec, SR);
        assert!(errors.is_empty(), "{errors:?}");
        b.midi(0x90, note, 127, 0);
        let y = run(&mut b, 8 * SR as usize);
        (y, 440.0 * ((f32::from(note) - 69.0) / 12.0).exp2(), b)
    }

    fn wav(name: &str, s: &[f32]) -> String {
        let path =
            std::env::temp_dir().join(format!("prowave-808-{}-{name}.wav", std::process::id()));
        let stereo: Vec<f32> = s.iter().flat_map(|x| [*x, *x]).collect();
        crate::flex::write_wav(&path, &stereo, SR).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_one_shot_measured_an_octave_high_still_loops_without_a_flutter() {
        let sustain = |sample: String, root_hz: f32| Bass808Spec {
            sample,
            root_hz,
            params: Bass808Params {
                sustain: true,
                decay_s: 60.0,
                ..clean()
            },
        };
        // 45 Hz with its 2nd harmonic twice as loud: measured as 90 Hz.
        let loud_second = wav("45hz", &one_shot_at(45.0, 2.0, 2.0));
        let (s, measured, _) = prepared(&loud_second, SR, 0.0).unwrap();
        println!("45 Hz with a loud 2nd harmonic measures {measured:?}");
        assert!((measured.unwrap() - 90.0).abs() < 0.5);
        let lp = SustainLoop::new(&s, SR, 90.0).unwrap();
        println!("its loop at 90 Hz: likeness {:.5}", lp.likeness);
        assert!(lp.likeness > 0.999);
        // The root left as measured, and set right by hand: either way, held 8 s, no swing.
        for root in [0.0, 45.0] {
            let (y, _, b) = hold(&sustain(loud_second.clone(), root), 69);
            assert!(b.voice.active);
            let db = swing_db(&y, 45.0, 2, 8);
            println!("45 Hz one-shot, root {root}: swing {db:.3} dB");
            assert!(db < 0.5, "{db}");
        }
        // 20.6 Hz, below what the measurement looks at: its 2nd harmonic is found (41.2 Hz).
        let low = wav("20hz", &one_shot_at(20.6, 0.8, 3.0));
        let (_, measured, _) = prepared(&low, SR, 0.0).unwrap();
        println!("20.6 Hz one-shot measures {measured:?}");
        for root in [0.0, 20.6] {
            let spec = sustain(low.clone(), root);
            let (y, _, b) = hold(&spec, 69);
            let lp = b.sustain.as_ref().unwrap();
            let db = swing_db(&y, 20.6 * 440.0 / b.root_hz, 2, 8);
            println!(
                "20.6 Hz one-shot, root {root}: loop {:.3} s, likeness {:.5}, swing {db:.3} dB",
                lp.buf.len() as f32 / SR as f32,
                lp.likeness
            );
            assert!(db < 0.5, "{db}");
            assert!(can_sustain(&spec, SR));
        }
        // A loop cut at a pitch that is no harmonic of the sound (1.25 times it): either it
        // spans a whole number of the real periods (and plays at the real pitch, speed 1), or
        // there is none.
        if let Some(lp) = SustainLoop::new(&one_shot_at(45.0, 0.0, 2.0), SR, 56.25) {
            println!(
                "at 1.25 times the pitch: likeness {:.5}, speed {:.5}",
                lp.likeness, lp.ratio
            );
            assert!(lp.likeness >= LOOP_LIKENESS && (lp.ratio - 1.0).abs() < 1e-3);
        }
        // A sound with no pitch at all (falling noise): no two stretches alike, no loop.
        let mut seed = 1u32;
        let noise: Vec<f32> = (0..2 * SR as usize)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
            } * (-(i as f32) / SR as f32).exp())
            .collect();
        assert!(SustainLoop::new(&noise, SR, 45.0).is_none());
        let _ = std::fs::remove_file(loud_second);
        let _ = std::fs::remove_file(low);
    }

    #[test]
    fn the_sustain_key_knows_when_no_loop_fits() {
        // 0.1 s: too short for even two periods after the attack.
        let short = wav("short", &one_shot_at(45.0, 0.0, 0.1));
        let long = wav("long", &one_shot_at(45.0, 0.0, 2.0));
        let spec = |sample: &str| Bass808Spec {
            sample: sample.into(),
            ..Bass808Spec::default()
        };
        assert!(!can_sustain(&spec(&short), SR));
        assert!(can_sustain(&spec(&long), SR));
        assert!(!can_sustain(&Bass808Spec::default(), SR));
        let _ = std::fs::remove_file(short);
        let _ = std::fs::remove_file(long);
    }

    #[test]
    fn a_rebuilt_808_reuses_its_prepared_one_shot() {
        let path = wav("cache", &one_shot_at(45.0, 0.0, 2.0));
        let spec = Bass808Spec {
            sample: path.clone(),
            ..Bass808Spec::default()
        };
        let (a, _) = Bass808::new(&spec, SR);
        let (b, _) = Bass808::new(&spec, SR);
        let same = |x: &Option<Arc<[f32]>>, y: &Option<Arc<[f32]>>| match (x, y) {
            (Some(x), Some(y)) => Arc::ptr_eq(x, y),
            _ => false,
        };
        assert!(same(&a.sample, &b.sample), "read again");
        assert!(Arc::ptr_eq(
            a.sustain.as_ref().unwrap(),
            b.sustain.as_ref().unwrap()
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn all_sound_off_cuts_and_all_notes_off_releases() {
        let long = Bass808Params {
            decay_s: 60.0,
            release_s: 20.0,
            ..clean()
        };
        let mut b = synth(long);
        b.midi(0x90, 36, 127, 0);
        run(&mut b, 4800);
        b.midi(0xb0, 120, 0, 0);
        let y = run(&mut b, 480);
        assert!(!b.voice.active);
        assert_eq!(peak(&y[200..]), 0.0, "silent after the 4 ms fade");
        let (d1, _) = roughness(&y);
        assert!(d1 < 0.02, "no click: {d1}");
        let mut b = synth(long);
        b.midi(0x90, 36, 127, 0);
        run(&mut b, 4800);
        b.midi(0xb0, 123, 0, 0);
        let y = run(&mut b, SR as usize / 2);
        assert!(b.voice.active && b.voice.released && peak(&y[12_000..]) > 0.5);
    }

    #[test]
    fn a_long_slide_over_a_small_interval_lands_without_a_jump() {
        for rate in [48_000u32, 192_000] {
            let (mut b, _) = Bass808::new(
                &Bass808Spec {
                    params: Bass808Params {
                        glide_ms: 1000.0,
                        ..clean()
                    },
                    ..Bass808Spec::default()
                },
                rate,
            );
            b.midi(0x90, 36, 127, 0);
            let mut l = vec![0f32; 64];
            let mut r = vec![0f32; 64];
            b.process((&[], &[]), (&mut l, &mut r));
            b.midi(0x90, 37, 127, 0);
            let mut pitches = Vec::with_capacity(rate as usize + 64);
            let (mut l, mut r) = (vec![0f32; 1], vec![0f32; 1]);
            for _ in 0..rate + 64 {
                b.process((&[], &[]), (&mut l, &mut r));
                pitches.push(b.voice.pitch);
            }
            let biggest = pitches.windows(2).fold(0f32, |m, w| m.max(w[1] - w[0]));
            let even = 1.0 / rate as f32;
            println!(
                "{rate} Hz: biggest move a frame {:.3e} st, an even slide's {even:.3e}",
                biggest
            );
            assert_eq!(*pitches.last().unwrap(), 37.0);
            assert!(biggest < 4.0 * even, "{biggest} {even}");
            assert!(pitches.windows(2).all(|w| w[1] >= w[0]));
        }
    }
}
