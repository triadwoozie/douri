// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The synthesizer is ported from LMMS's Monstro
// (plugins/Monstro/Monstro.cpp and Monstro.h; Copyright (c) 2014 Vesa Kivimäki), with the parts
// of LMMS it uses: its band-limited wavetables (src/core/BandLimitedWave.cpp and
// include/BandLimitedWave.h; Copyright (c) 2014 Vesa Kivimäki), Oscillator's waveforms
// (include/Oscillator.h; Copyright (c) 2004-2014 Tobias Doerffel) and helpers of
// include/lmms_math.h and include/interpolation.h (Copyright (c) 2004-2008 Tobias Doerffel);
// GPL-2.0-or-later, taken here under GPL-3.0-or-later; https://github.com/LMMS/lmms at commit
// f085f1e. Pro Wave's voices, parameter table and tests are its own.

//! Monstro: three oscillators, two envelopes, two LFOs and a modulation matrix.
//!
//! Logic Pro equivalent: ES2 (three oscillators and a modulation router).
//!
//! Oscillator 1 is a pulse wave that can restart (hard sync) or turn over (reverse sync)
//! oscillators 2 and 3 when it rises or falls. Oscillator 2 has fifteen waves; oscillator 3 has
//! two, mixed by Sub Mix. Oscillator 2 is mixed in, or modulates oscillator 3's amplitude,
//! frequency or phase (Osc 2+3 Modulation). Each oscillator's left and right are separate
//! oscillators: Fine Left and Fine Right detune them, Stereo Phase sets them apart.
//!
//! The matrix sends each envelope and LFO to each oscillator's volume, phase and pitch,
//! oscillator 1's pulse width and oscillator 3's sub mix, by an amount from -1 to 1 (the
//! values here are thousandths). Nothing is routed at first, so a note sounds at full volume
//! while its key is down, as in the original: the envelopes shape the sound only through the
//! matrix. Envelopes have pre-delay, attack, hold, decay, sustain, release and a slope that
//! bends every stage; LFOs have eleven waves, an attack (fade-in) and a rate given as the
//! length of one cycle.
//!
//! Each note is its own voice (eight at once). A voice lets go when its key does: both
//! envelopes go to their release, and it ends when both have finished, and at least 1.5 ms
//! after the key. (At commit f085f1e the original asks LMMS to keep a note for 1.5 ms after
//! its key whatever the releases are, because it reads the envelopes' per-sample steps where
//! their lengths were meant; the release lengths are what it meant, and what it did before.)
//! The LFO's fade-in grows sample by sample; the original's steps once a buffer. Pulse width
//! under modulation is kept between 25% and 100%, as the original keeps it (its limits are
//! written in percent where the width is a fraction). Velocity
//! scales the voice, as LMMS's track does, and Level is Pro Wave's.

use super::voices::Voices;
use super::NativeInst;
use crate::native_fx::NativeParam;
use std::f32::consts::PI;
use std::sync::OnceLock;

pub const ID: &str = "prowave:monstro";
pub const NAME: &str = "Monstro";

// Oscillator 1.
pub const O1_VOL: usize = 0;
pub const O1_PAN: usize = 1;
pub const O1_CRS: usize = 2;
pub const O1_FTL: usize = 3;
pub const O1_FTR: usize = 4;
pub const O1_SPO: usize = 5;
pub const O1_PW: usize = 6;
pub const O1_SSR: usize = 7;
pub const O1_SSF: usize = 8;
// Oscillator 2.
pub const O2_VOL: usize = 9;
pub const O2_PAN: usize = 10;
pub const O2_CRS: usize = 11;
pub const O2_FTL: usize = 12;
pub const O2_FTR: usize = 13;
pub const O2_SPO: usize = 14;
pub const O2_WAVE: usize = 15;
pub const O2_SYNC_H: usize = 16;
pub const O2_SYNC_R: usize = 17;
// Oscillator 3.
pub const O3_VOL: usize = 18;
pub const O3_PAN: usize = 19;
pub const O3_CRS: usize = 20;
pub const O3_SPO: usize = 21;
pub const O3_SUB: usize = 22;
pub const O3_WAVE1: usize = 23;
pub const O3_WAVE2: usize = 24;
pub const O3_SYNC_H: usize = 25;
pub const O3_SYNC_R: usize = 26;
/// LFO `n` (0 or 1) starts at `LFO + n * LFO_PARAMS`: Wave, Attack, Rate, Phase.
pub const LFO: usize = 27;
pub const LFO_PARAMS: usize = 4;
/// Envelope `n` starts at `ENV + n * ENV_PARAMS`: Pre-delay, Attack, Hold, Decay, Sustain,
/// Release, Slope.
pub const ENV: usize = 35;
pub const ENV_PARAMS: usize = 7;
pub const O23_MOD: usize = 49;
/// The matrix: target `t` (Vol 1-3, Phase 1-3, Pitch 1-3, PW 1, Sub 3) from source `s` (Env 1,
/// Env 2, LFO 1, LFO 2) is `MATRIX + t * 4 + s`.
pub const MATRIX: usize = 50;
pub const TARGETS: usize = 11;
pub const LEVEL: usize = MATRIX + TARGETS * 4;
pub const COUNT: usize = LEVEL + 1;

/// Matrix targets.
pub const T_VOL: usize = 0;
pub const T_PHS: usize = 3;
pub const T_PIT: usize = 6;
pub const T_PW: usize = 9;
pub const T_SUB: usize = 10;

pub const WAVES: [&str; 15] = [
    "Sine",
    "Triangle",
    "Saw",
    "Ramp",
    "Square",
    "Moog Saw",
    "Soft Square",
    "Abs Sine",
    "Exponential",
    "White Noise",
    "Digital Triangle",
    "Digital Saw",
    "Digital Ramp",
    "Digital Square",
    "Digital Moog Saw",
];
pub const LFO_WAVES: [&str; 11] = [
    "Sine",
    "Triangle",
    "Saw",
    "Ramp",
    "Square",
    "Moog Saw",
    "Soft Square",
    "Abs Sine",
    "Exponential",
    "Random",
    "Random Smooth",
];
pub const MODS: [&str; 4] = ["Mix", "AM", "FM", "PM"];
const OFF_ON: [&str; 2] = ["Off", "On"];

const WAVE_SINE: i32 = 0;
const WAVE_TRI: i32 = 1;
const WAVE_SAW: i32 = 2;
const WAVE_RAMP: i32 = 3;
const WAVE_SQR: i32 = 4;
const WAVE_MOOG: i32 = 5;
const WAVE_SQRSOFT: i32 = 6;
const WAVE_SINABS: i32 = 7;
const WAVE_EXP: i32 = 8;
const WAVE_NOISE: i32 = 9;
const WAVE_TRI_D: i32 = 10;
const WAVE_SAW_D: i32 = 11;
const WAVE_RAMP_D: i32 = 12;
const WAVE_SQR_D: i32 = 13;
const WAVE_MOOG_D: i32 = 14;
const WAVE_RANDOM: i32 = 9;
const WAVE_RANDOM_SMOOTH: i32 = 10;

const MOD_MIX: i32 = 0;
const MOD_AM: i32 = 1;
const MOD_FM: i32 = 2;
const MOD_PM: i32 = 3;

const MODCLIP: f32 = 2.0;
const MIN_FREQ: f32 = 18.0;
const MAX_FREQ: f32 = 48000.0;
const INTEGRATOR: f32 = 3.0 / 7.0;
const FM_AMOUNT: f32 = 0.25;
const PW_MIN: f32 = 0.25;
const PW_MAX: f32 = 100.0 - PW_MIN;

/// Voices at once.
pub const MAX_VOICES: usize = 8;
/// The least time (seconds) a voice sounds after its key: `desiredReleaseTimeMs`'s floor.
const RELEASE_FLOOR: f32 = 0.0015;

enum Kind {
    Knob(i32, i32, i32, &'static str),
    Choice(i32, &'static [&'static str]),
}

/// Every parameter's name and range: the original's, with fractions as whole numbers (pulse
/// width in hundredths of a percent, sustain, slope and the matrix in thousandths, LFO rate in
/// tenths of a ms).
fn table() -> Vec<(String, Kind)> {
    use Kind::{Choice, Knob};
    let mut t: Vec<(String, Kind)> = Vec::with_capacity(COUNT);
    for n in 1..=3 {
        let o = |s: &str| format!("Osc {n} {s}");
        t.push((o("Volume"), Knob(0, 200, 33, "%")));
        t.push((o("Pan"), Knob(-100, 100, 0, "")));
        t.push((o("Coarse"), Knob(-24, 24, 0, "st")));
        if n < 3 {
            t.push((o("Fine Left"), Knob(-100, 100, 0, "ct")));
            t.push((o("Fine Right"), Knob(-100, 100, 0, "ct")));
        }
        t.push((o("Stereo Phase"), Knob(-180, 180, 0, "°")));
        match n {
            1 => {
                t.push((o("Pulse Width"), Knob(25, 9975, 5000, "%")));
                t.push((o("Sync on Rise"), Choice(0, &OFF_ON)));
                t.push((o("Sync on Fall"), Choice(0, &OFF_ON)));
            }
            2 => {
                t.push((o("Wave"), Choice(0, &WAVES)));
                t.push((o("Hard Sync"), Choice(0, &OFF_ON)));
                t.push((o("Reverse Sync"), Choice(0, &OFF_ON)));
            }
            _ => {
                t.push((o("Sub Mix"), Knob(-100, 100, 0, "")));
                t.push((o("Wave 1"), Choice(0, &WAVES)));
                t.push((o("Wave 2"), Choice(0, &WAVES)));
                t.push((o("Hard Sync"), Choice(0, &OFF_ON)));
                t.push((o("Reverse Sync"), Choice(0, &OFF_ON)));
            }
        }
    }
    for n in 1..=2 {
        let l = |s: &str| format!("LFO {n} {s}");
        t.push((l("Wave"), Choice(0, &LFO_WAVES)));
        t.push((l("Attack"), Knob(0, 2000, 0, "ms")));
        t.push((l("Rate"), Knob(1, 100_000, 10, "ms")));
        t.push((l("Phase"), Knob(-180, 180, 0, "°")));
    }
    for n in 1..=2 {
        let e = |s: &str| format!("Env {n} {s}");
        t.push((e("Pre-delay"), Knob(0, 2000, 0, "ms")));
        t.push((e("Attack"), Knob(0, 2000, 0, "ms")));
        t.push((e("Hold"), Knob(0, 4000, 0, "ms")));
        t.push((e("Decay"), Knob(0, 4000, 0, "ms")));
        t.push((e("Sustain"), Knob(0, 1000, 1000, "")));
        t.push((e("Release"), Knob(0, 4000, 0, "ms")));
        t.push((e("Slope"), Knob(-1000, 1000, 0, "")));
    }
    t.push(("Osc 2+3 Modulation".to_string(), Choice(0, &MODS)));
    let targets = [
        ("Vol", 1),
        ("Vol", 2),
        ("Vol", 3),
        ("Phase", 1),
        ("Phase", 2),
        ("Phase", 3),
        ("Pitch", 1),
        ("Pitch", 2),
        ("Pitch", 3),
        ("PW", 1),
        ("Sub", 3),
    ];
    for (what, osc) in targets {
        for src in ["Env 1", "Env 2", "LFO 1", "LFO 2"] {
            t.push((format!("{what} Osc {osc} {src}"), Knob(-1000, 1000, 0, "")));
        }
    }
    t.push(("Level".to_string(), Knob(-48, 12, 0, "dB")));
    t
}

pub fn params() -> &'static [NativeParam] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    static PARAMS: OnceLock<Vec<NativeParam>> = OnceLock::new();
    PARAMS.get_or_init(|| {
        let names = NAMES.get_or_init(|| table().into_iter().map(|(n, _)| n).collect());
        table()
            .into_iter()
            .zip(names)
            .map(|((_, kind), name)| match kind {
                Kind::Knob(min, max, def, unit) => NativeParam::knob(name, min, max, def, unit),
                Kind::Choice(def, choices) => NativeParam::choice(name, 0, def, choices),
            })
            .collect()
    })
}

pub fn value_text(k: usize, v: i32) -> String {
    let Some(p) = params().get(k) else {
        return String::new();
    };
    if !p.choices.is_empty() {
        return p
            .choices
            .get(v.max(0) as usize)
            .copied()
            .unwrap_or("")
            .to_string();
    }
    match k {
        O1_PW => format!("{:.2}%", v as f32 / 100.0),
        k if (MATRIX..LEVEL).contains(&k) => format!("{:.3}", v as f32 / 1000.0),
        k if (ENV..O23_MOD).contains(&k) && matches!((k - ENV) % ENV_PARAMS, 4 | 6) => {
            format!("{:.3}", v as f32 / 1000.0)
        }
        k if (LFO..ENV).contains(&k) && (k - LFO) % LFO_PARAMS == 2 => {
            format!("{:.1}ms", v as f32 / 10.0)
        }
        _ => format!("{v}{}", p.unit),
    }
}

// ---- LMMS's helpers ----

/// `fraction`: the part after the point, with the sign of `x`.
#[inline]
fn fraction(x: f32) -> f32 {
    x - x.trunc()
}

/// `absFraction`: wrapped into 0 to 1.
#[inline]
fn abs_fraction(x: f32) -> f32 {
    x - x.floor()
}

/// `fastPow`: an approximation by the bits of the double.
#[inline]
fn fast_pow(a: f64, b: f64) -> f64 {
    let bits = a.to_bits();
    let hi = (bits >> 32) as i32;
    let hi = (b * f64::from(hi - 1_072_632_447) + 1_072_632_447.0) as i32;
    f64::from_bits(u64::from(hi as u32) << 32)
}

fn optimal_4p(v0: f32, v1: f32, v2: f32, v3: f32, x: f32) -> f32 {
    let z = x - 0.5;
    let (even1, odd1) = (v2 + v1, v2 - v1);
    let (even2, odd2) = (v3 + v0, v3 - v0);
    let c0 = even1 * 0.458_689_7 + even2 * 0.041_314_02;
    let c1 = odd1 * 0.480_680_25 + odd2 * 0.175_779_26;
    let c2 = even1 * -0.246_185 + even2 * 0.246_140_27;
    let c3 = odd1 * -0.360_309_26 + odd2 * 0.101_749_86;
    ((c3 * z + c2) * z + c1) * z + c0
}

fn cosinus(v0: f32, v1: f32, x: f32) -> f32 {
    let f = (1.0 - (x * PI).cos()) * 0.5;
    f * (v1 - v0) + v0
}

/// LMMS's `fastRand`: its own sequence, kept by the instrument.
#[derive(Clone, Copy)]
struct FastRand(u64);

impl FastRand {
    fn next(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12345);
        ((self.0 / 65536) % 32768) as i32
    }

    /// `Oscillator::noiseSample`: -1 to 1.
    fn noise(&mut self) -> f32 {
        -1.0 + self.next() as f32 * 2.0 * (1.0 / 32767.0)
    }
}

fn sin_sample(x: f32) -> f32 {
    (x * 2.0 * PI).sin()
}

fn triangle_sample(x: f32) -> f32 {
    let ph = abs_fraction(x);
    if ph <= 0.25 {
        ph * 4.0
    } else if ph <= 0.75 {
        2.0 - ph * 4.0
    } else {
        ph * 4.0 - 4.0
    }
}

fn saw_sample(x: f32) -> f32 {
    -1.0 + abs_fraction(x) * 2.0
}

fn square_sample(x: f32) -> f32 {
    if abs_fraction(x) > 0.5 {
        -1.0
    } else {
        1.0
    }
}

fn moog_saw_sample(x: f32) -> f32 {
    let ph = abs_fraction(x);
    if ph < 0.5 {
        -1.0 + ph * 4.0
    } else {
        1.0 - 2.0 * ph
    }
}

fn exp_sample(x: f32) -> f32 {
    let mut ph = abs_fraction(x);
    if ph > 0.5 {
        ph = 1.0 - ph;
    }
    -1.0 + 8.0 * ph * ph
}

// ---- BandLimitedWave ----

const MAXTBL: usize = 23;
const TLENS: [usize; MAXTBL + 1] = [
    2,
    3,
    2 << 1,
    3 << 1,
    2 << 2,
    3 << 2,
    2 << 3,
    3 << 3,
    2 << 4,
    3 << 4,
    2 << 5,
    3 << 5,
    2 << 6,
    3 << 6,
    2 << 7,
    3 << 7,
    2 << 8,
    3 << 8,
    2 << 9,
    3 << 9,
    2 << 10,
    3 << 10,
    2 << 11,
    3 << 11,
];

/// The band-limited waves: saw, square, triangle, moog saw; each a table for every length.
struct Waves([Vec<Vec<f32>>; 4]);
const BL_SAW: usize = 0;
const BL_SQUARE: usize = 1;
const BL_TRIANGLE: usize = 2;
const BL_MOOG: usize = 3;

/// The tables, made the way `BandLimitedWave::generateWaves` makes them when it has no files:
/// each a sum of the harmonics below half its length, normalized. (Here every harmonic's sine
/// is read from one cycle of the table's length, in double precision; the original works each
/// one out with a single-precision pi, which differs by less than a ten-thousandth.)
fn waves() -> &'static Waves {
    static WAVES_: OnceLock<Waves> = OnceLock::new();
    WAVES_.get_or_init(|| {
        let make = |first: usize, step: usize, amp: fn(usize) -> f64| -> Vec<Vec<f32>> {
            TLENS
                .iter()
                .map(|&len| {
                    let sines: Vec<f64> = (0..len)
                        .map(|k| (k as f64 / len as f64 * std::f64::consts::TAU).sin())
                        .collect();
                    let raw: Vec<f64> = (0..len)
                        .map(|ph| {
                            let (mut harm, mut s) = (first, 0.0f64);
                            loop {
                                let hlen = len as f64 / harm as f64;
                                s += amp(harm) * sines[(ph * harm) % len];
                                harm += step;
                                if hlen <= 2.0 {
                                    break;
                                }
                            }
                            s
                        })
                        .collect();
                    let max = raw.iter().fold(0.0f64, |m, v| m.max(v.abs()));
                    let max = if max > 0.0 { max } else { 1.0 };
                    raw.iter()
                        .map(|v| (*v as f32 as f64 / max) as f32)
                        .collect()
                })
                .collect()
        };
        let saw = make(1, 1, |h| -1.0 / h as f64);
        let square = make(1, 2, |h| 1.0 / h as f64);
        // Every other odd harmonic half a cycle round: its sign turned over.
        let triangle = make(1, 2, |h| {
            let a = 1.0 / (h * h) as f64;
            if (h + 1) % 4 == 0 {
                -a
            } else {
                a
            }
        });
        let moog = TLENS
            .iter()
            .enumerate()
            .map(|(i, &len)| {
                (0..len)
                    .map(|ph| {
                        let sawph = (ph + (len as f32 * 0.75) as usize) % len;
                        (saw[i][sawph] + triangle[i][ph]) * 0.5
                    })
                    .collect()
            })
            .collect();
        Waves([saw, square, triangle, moog])
    })
}

impl Waves {
    /// `BandLimitedWave::oscillate`.
    fn oscillate(&self, ph: f32, wavelen: f32, wave: usize) -> f32 {
        let mut t = 0;
        while t < MAXTBL && wavelen >= TLENS[t + 1] as f32 {
            t += 1;
        }
        let tlen = TLENS[t];
        let table = &self.0[wave][t];
        let ph = fraction(ph);
        let lookupf = ph * tlen as f32;
        let lookup = (lookupf.max(0.0) as usize) % tlen;
        let ip = fraction(lookupf);
        let s1 = table[lookup];
        let s2 = table[(lookup + 1) % tlen];
        let lm = if lookup == 0 { tlen - 1 } else { lookup - 1 };
        let s0 = table[lm];
        let s3 = table[(lookup + 2) % tlen];
        optimal_4p(s0, s1, s2, s3, ip)
    }
}

/// `MonstroSynth::oscillate`.
fn oscillate(w: &Waves, wave: i32, ph: f32, wavelen: f32, rng: &mut FastRand) -> f32 {
    match wave {
        WAVE_SINE => sin_sample(ph),
        WAVE_TRI => w.oscillate(ph, wavelen, BL_TRIANGLE),
        WAVE_SAW => w.oscillate(ph, wavelen, BL_SAW),
        WAVE_RAMP => -w.oscillate(ph, wavelen, BL_SAW),
        WAVE_SQR => w.oscillate(ph, wavelen, BL_SQUARE),
        WAVE_SQRSOFT => {
            let ph = fraction(ph);
            if ph < 0.1 {
                sin_sample(ph * 5.0 + 0.75)
            } else if ph < 0.5 {
                1.0
            } else if ph < 0.6 {
                sin_sample(ph * 5.0 + 0.75)
            } else {
                -1.0
            }
        }
        WAVE_MOOG => w.oscillate(ph, wavelen, BL_MOOG),
        WAVE_SINABS => sin_sample(ph).abs(),
        WAVE_EXP => exp_sample(ph),
        WAVE_NOISE => rng.noise(),
        WAVE_TRI_D => triangle_sample(ph),
        WAVE_SAW_D => saw_sample(ph),
        WAVE_RAMP_D => -saw_sample(ph),
        WAVE_SQR_D => square_sample(ph),
        WAVE_MOOG_D => moog_saw_sample(ph),
        _ => 0.0,
    }
}

// ---- The instrument's settings, worked out (the original's `update…` slots) ----

/// What the parameters mean at the rate in use.
#[derive(Clone, Copy, Default)]
struct Derived {
    vol: [[f32; 2]; 3],
    /// Oscillator 1 and 2's frequency ratios for left and right; oscillator 3's.
    freq: [[f32; 2]; 3],
    po: [[f32; 2]; 3],
    env_pre: [f32; 2],
    env_att: [f32; 2],
    env_hold: [f32; 2],
    env_dec: [f32; 2],
    env_rel: [f32; 2],
    env_sus: [f32; 2],
    slope: [f32; 2],
    lfo_att: [f32; 2],
    lfo_rate: [f32; 2],
    lfo_po: [f32; 2],
    lfo_wave: [i32; 2],
    /// Matrix amounts, with the original's scaling of each.
    matrix: [[f32; 4]; TARGETS],
    pw: f32,
    sub: f32,
    integrator: f32,
    fm_correction: f32,
    counter_max: i32,
    gain: f32,
}

fn derive(p: &[i32; COUNT], rate: f32) -> Derived {
    let f = |k: usize| p[k] as f32;
    let left =
        |vol: f32, pan: f32| (if pan <= 0.0 { 1.0 } else { 1.0 - pan / 100.0 }) * vol / 100.0;
    let right =
        |vol: f32, pan: f32| (if pan >= 0.0 { 1.0 } else { 1.0 + pan / 100.0 }) * vol / 100.0;
    let mut d = Derived::default();
    for (n, (vol, pan)) in [(O1_VOL, O1_PAN), (O2_VOL, O2_PAN), (O3_VOL, O3_PAN)]
        .into_iter()
        .enumerate()
    {
        d.vol[n] = [left(f(vol), f(pan)), right(f(vol), f(pan))];
    }
    for (n, (crs, ftl, ftr)) in [(O1_CRS, O1_FTL, O1_FTR), (O2_CRS, O2_FTL, O2_FTR)]
        .into_iter()
        .enumerate()
    {
        let c = (f(crs) / 12.0).exp2();
        d.freq[n] = [c * (f(ftl) / 1200.0).exp2(), c * (f(ftr) / 1200.0).exp2()];
    }
    let c3 = (f(O3_CRS) / 12.0).exp2();
    d.freq[2] = [c3, c3];
    for (n, spo) in [O1_SPO, O2_SPO, O3_SPO].into_iter().enumerate() {
        d.po[n] = [f(spo) / 720.0, -f(spo) / 720.0];
    }
    let step = |ms: f32| {
        if ms == 0.0 {
            1.0
        } else {
            1.0 / (ms / 1000.0) / rate
        }
    };
    for e in 0..2 {
        let b = ENV + e * ENV_PARAMS;
        d.env_pre[e] = step(f(b));
        d.env_att[e] = step(f(b + 1));
        d.env_hold[e] = step(f(b + 2));
        d.env_dec[e] = step(f(b + 3));
        d.env_sus[e] = f(b + 4) / 1000.0;
        d.env_rel[e] = step(f(b + 5));
        d.slope[e] = (std::f32::consts::LN_10 * -(f(b + 6) / 1000.0)).exp();
        let l = LFO + e * LFO_PARAMS;
        d.lfo_wave[e] = p[l];
        d.lfo_att[e] = (f(l + 1) * rate / 1000.0).trunc();
        d.lfo_rate[e] = f(l + 2) / 10.0 * 0.001 * rate;
        d.lfo_po[e] = f(l + 3) / 360.0;
    }
    for t in 0..TARGETS {
        for s in 0..4 {
            let v = f(MATRIX + t * 4 + s) / 1000.0;
            // Envelopes to pitch count double; LFOs to phase, pulse width and sub count half.
            d.matrix[t][s] = match (t, s) {
                (T_PIT..=8, 0 | 1) => v * 2.0,
                (T_PHS..=5 | T_PW | T_SUB, 2 | 3) => v * 0.5,
                _ => v,
            };
        }
    }
    d.pw = f(O1_PW) / 100.0 * 0.01;
    d.sub = (f(O3_SUB) + 100.0) / 200.0;
    d.integrator = 0.5 - (0.5 - INTEGRATOR) * 44100.0 / rate;
    d.fm_correction = 44100.0 / rate * FM_AMOUNT;
    d.counter_max = (rate as i32 * 5) / 44100;
    d.gain = 10f32.powf(f(LEVEL) / 20.0);
    d
}

// ---- A voice: `MonstroSynth` ----

#[derive(Clone, Copy, Default)]
struct Voice {
    hz: f32,
    velocity: f32,
    /// Oscillator phases (without their offsets), left and right.
    phase: [[f32; 2]; 3],
    env_phase: [f32; 2],
    lfo_phase: [f32; 2],
    lfo_last: [f32; 2],
    lfo_next: [f32; 2],
    osc1_last: [f32; 2],
    out_last: [f32; 2],
    ph2_last: [f32; 2],
    ph3_last: [f32; 2],
    invert2: [bool; 2],
    invert3: [bool; 2],
    counter2: [i32; 2],
    counter3: [i32; 2],
    /// Frames played since the note began.
    played: u64,
    released: bool,
    /// Frames since the key went up.
    since_release: u64,
}

impl Voice {
    fn start(hz: f32, velocity: f32, rng: &mut FastRand) -> Voice {
        Voice {
            hz,
            velocity,
            lfo_next: [rng.noise(), rng.noise()],
            ..Voice::default()
        }
    }

    /// `calcSlope`.
    fn slope(d: &Derived, i: usize, s: f32) -> f32 {
        if d.slope[i] == 1.0 || s == 0.0 {
            return s;
        }
        fast_pow(f64::from(s), f64::from(d.slope[i])) as f32
    }

    /// The envelopes' and LFOs' values for this frame (`updateModulators`, one frame).
    fn modulators(&mut self, d: &Derived, w: &Waves, rng: &mut FastRand) -> ([f32; 2], [f32; 2]) {
        let mut env = [0.0f32; 2];
        let mut lfo = [0.0f32; 2];
        let frame = self.played;
        for i in 0..2 {
            let ph = abs_fraction(self.lfo_phase[i] + d.lfo_po[i]);
            let rate_frames = (d.lfo_rate[i] as i64).max(1) as u64;
            lfo[i] = match d.lfo_wave[i] {
                WAVE_SINE => sin_sample(ph),
                WAVE_TRI => triangle_sample(ph),
                WAVE_SAW => saw_sample(ph),
                WAVE_RAMP => -saw_sample(ph),
                WAVE_SQR => square_sample(ph),
                WAVE_SQRSOFT | WAVE_SINABS => oscillate(w, d.lfo_wave[i], ph, 0.0, rng),
                WAVE_MOOG => moog_saw_sample(ph),
                WAVE_EXP => exp_sample(ph),
                WAVE_RANDOM => {
                    if frame.is_multiple_of(rate_frames) {
                        self.lfo_last[i] = rng.noise();
                    }
                    self.lfo_last[i]
                }
                WAVE_RANDOM_SMOOTH => {
                    let tm = frame % rate_frames;
                    if tm == 0 {
                        self.lfo_last[i] = self.lfo_next[i];
                        self.lfo_next[i] = rng.noise();
                    }
                    cosinus(
                        self.lfo_last[i],
                        self.lfo_next[i],
                        tm as f32 / d.lfo_rate[i],
                    )
                }
                _ => 0.0,
            };
            self.lfo_phase[i] = abs_fraction(self.lfo_phase[i] + 1.0 / d.lfo_rate[i]);
            // The fade-in.
            if (frame as f32) < d.lfo_att[i] {
                lfo[i] *= frame as f32 / d.lfo_att[i];
            }

            // The envelope: released, it moves to the release at the level it has.
            let e = &mut self.env_phase[i];
            if *e < 4.0 && self.released {
                *e = if *e < 1.0 {
                    5.0
                } else if *e < 2.0 {
                    5.0 - fraction(*e)
                } else if *e < 3.0 {
                    4.0
                } else {
                    4.0 + fraction(*e)
                };
            }
            env[i] = if *e < 1.0 {
                *e = (*e + d.env_pre[i]).min(1.0);
                0.0
            } else if *e < 2.0 {
                let v = Self::slope(d, i, fraction(*e));
                *e = (*e + d.env_att[i]).min(2.0);
                v
            } else if *e < 3.0 {
                *e = (*e + d.env_hold[i]).min(3.0);
                1.0
            } else if *e < 4.0 {
                let s = Self::slope(d, i, 1.0 - fraction(*e));
                if s <= d.env_sus[i] {
                    d.env_sus[i]
                } else {
                    *e = (*e + d.env_dec[i]).min(4.0 - d.env_sus[i]);
                    // Over the release when the sustain is zero.
                    if *e == 4.0 {
                        *e = 5.0;
                    }
                    s
                }
            } else if *e < 5.0 {
                let v = Self::slope(d, i, 1.0 - fraction(*e));
                *e += d.env_rel[i];
                v
            } else {
                0.0
            };
        }
        (env, lfo)
    }

    /// Whether both envelopes are over.
    fn envelopes_done(&self) -> bool {
        self.env_phase.iter().all(|e| *e >= 5.0)
    }
}

/// The matrix's sum for target `t` (`modulateabs` / `modulatephs`).
#[inline]
fn add_mod(d: &Derived, t: usize, env: &[f32; 2], lfo: &[f32; 2]) -> f32 {
    let m = &d.matrix[t];
    let mut v = 0.0;
    if m[0] != 0.0 {
        v += env[0] * m[0];
    }
    if m[1] != 0.0 {
        v += env[1] * m[1];
    }
    if m[2] != 0.0 {
        v += lfo[0] * m[2];
    }
    if m[3] != 0.0 {
        v += lfo[1] * m[3];
    }
    v
}

/// `modulatevol`.
#[inline]
fn mod_vol(d: &Derived, t: usize, car: f32, env: &[f32; 2], lfo: &[f32; 2]) -> f32 {
    let m = &d.matrix[t];
    let mut car = car;
    for (m, e) in m[..2].iter().zip(env) {
        if *m > 0.0 {
            car *= 1.0 - m + m * e;
        }
        if *m < 0.0 {
            car *= 1.0 + m * e;
        }
    }
    for (m, l) in m[2..].iter().zip(lfo) {
        if *m != 0.0 {
            car *= 1.0 + m * l;
        }
    }
    car.clamp(-MODCLIP, MODCLIP)
}

fn any_mod(d: &Derived, t: usize) -> bool {
    d.matrix[t].iter().any(|v| *v != 0.0)
}

/// The instrument.
pub struct Monstro {
    rate: f32,
    applied: [i32; COUNT],
    derived: Derived,
    voices: [Voice; MAX_VOICES],
    alloc: Voices<MAX_VOICES>,
    rng: FastRand,
    bend: f32,
    waves: &'static Waves,
}

impl Monstro {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let rate = rate.max(1) as f32;
        let mut applied = [0; COUNT];
        for (k, (a, p)) in applied.iter_mut().zip(params()).enumerate() {
            *a = p.stored(values.get(k).copied().unwrap_or(p.default));
        }
        Monstro {
            rate,
            applied,
            derived: derive(&applied, rate),
            voices: [Voice::default(); MAX_VOICES],
            alloc: Voices::new(),
            rng: FastRand(1),
            bend: 0.0,
            waves: waves(),
        }
    }
}

impl NativeInst for Monstro {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = params().get(index) {
            self.applied[index] = p.stored(value);
            self.derived = derive(&self.applied, self.rate);
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        let i = self.alloc.start(note, MAX_VOICES);
        let hz = 440.0 * ((f32::from(note) - 69.0) / 12.0).exp2();
        self.voices[i] = Voice::start(hz, f32::from(velocity) / 127.0, &mut self.rng);
    }

    fn note_off(&mut self, note: u8) {
        if let Some(i) = self.alloc.stop(note) {
            self.voices[i].released = true;
        }
    }

    fn all_notes_off(&mut self) {
        let voices = &mut self.voices;
        self.alloc.stop_all(|i| voices[i].released = true);
    }

    fn all_sound_off(&mut self) {
        self.alloc.clear();
    }

    fn bend(&mut self, semitones: f32) {
        self.bend = semitones;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        let d = self.derived;
        let p = &self.applied;
        let w = self.waves;
        let rate = self.rate;
        let bend = (self.bend / 12.0).exp2();
        let floor = (RELEASE_FLOOR * rate) as u64;
        let (o2w, o3w1, o3w2) = (p[O2_WAVE], p[O3_WAVE1], p[O3_WAVE2]);
        let omod = p[O23_MOD];
        let (o1ssr, o1ssf) = (p[O1_SSR] != 0, p[O1_SSF] != 0);
        let (o2sync, o3sync) = (p[O2_SYNC_H] != 0, p[O3_SYNC_H] != 0);
        let (o2syncr, o3syncr) = (p[O2_SYNC_R] != 0, p[O3_SYNC_R] != 0);
        let o1pw_mod = any_mod(&d, T_PW);
        let o1p_mod = any_mod(&d, T_PHS);
        let o1f_mod = any_mod(&d, T_PIT);
        let o1v_mod = any_mod(&d, T_VOL);
        let o2p_mod = any_mod(&d, T_PHS + 1);
        let o2f_mod = any_mod(&d, T_PIT + 1);
        let o2v_mod = any_mod(&d, T_VOL + 1);
        let o3p_mod = any_mod(&d, T_PHS + 2);
        let o3f_mod = any_mod(&d, T_PIT + 2);
        let o3v_mod = any_mod(&d, T_VOL + 2);
        let o3s_mod = any_mod(&d, T_SUB);
        let rng = &mut self.rng;
        for (vi, v) in self.voices.iter_mut().enumerate() {
            if !self.alloc.is_busy(vi) {
                continue;
            }
            let hz = v.hz * bend;
            let base = [
                [d.freq[0][0] * hz, d.freq[0][1] * hz],
                [d.freq[1][0] * hz, d.freq[1][1] * hz],
                [d.freq[2][0] * hz, d.freq[2][1] * hz],
            ];
            let gain = v.velocity * d.gain;
            // The working phases carry their offsets, as in the original.
            let mut ph: [[f32; 2]; 3] =
                std::array::from_fn(|o| std::array::from_fn(|s| v.phase[o][s] + d.po[o][s]));
            for (l_out, r_out) in left.iter_mut().zip(right.iter_mut()) {
                let (env, lfo) = v.modulators(&d, w, rng);
                let freq_mod = |car: f32, t: usize| {
                    (car * add_mod(&d, t, &env, &lfo).exp2()).clamp(MIN_FREQ, MAX_FREQ)
                };
                let mut out = [0.0f32; 2];
                let mut o2 = [0.0f32; 2];
                // Oscillator 1: the pulse.
                let mut pw = d.pw;
                if o1pw_mod {
                    pw = (pw + add_mod(&d, T_PW, &env, &lfo)).clamp(PW_MIN, PW_MAX);
                }
                let mut o1 = [0.0f32; 2];
                for s in 0..2 {
                    let f = if o1f_mod {
                        freq_mod(base[0][s], T_PIT)
                    } else {
                        base[0][s]
                    };
                    let mut x = ph[0][s];
                    if o1p_mod {
                        x += add_mod(&d, T_PHS, &env, &lfo);
                    }
                    let mut o = if abs_fraction(x) < pw { 1.0 } else { -1.0 };
                    let last = v.osc1_last[s];
                    let rose = o > last;
                    let fell = o < last;
                    let sync = (o1ssr && rose) || (o1ssf && fell);
                    if sync {
                        if o2sync {
                            ph[1][s] = d.po[1][s];
                            v.counter2[s] = d.counter_max;
                        }
                        if o3sync {
                            ph[2][s] = d.po[2][s];
                            v.counter3[s] = d.counter_max;
                        }
                        if o2syncr {
                            v.invert2[s] = !v.invert2[s];
                            v.counter2[s] = d.counter_max;
                        }
                        if o3syncr {
                            v.invert3[s] = !v.invert3[s];
                            v.counter3[s] = d.counter_max;
                        }
                    }
                    v.osc1_last[s] = o;
                    // A very simple cap on the step.
                    if last != o {
                        o = 0.0;
                    }
                    o *= d.vol[0][s];
                    if o1v_mod {
                        o = mod_vol(&d, T_VOL, o, &env, &lfo);
                    }
                    o1[s] = o;
                    ph[0][s] += 1.0 / (rate / f);
                }
                // Oscillator 2.
                for s in 0..2 {
                    let f = if o2f_mod {
                        freq_mod(base[1][s], T_PIT + 1)
                    } else {
                        base[1][s]
                    };
                    let mut x = ph[1][s];
                    if o2p_mod {
                        x += add_mod(&d, T_PHS + 1, &env, &lfo);
                    }
                    let x = abs_fraction(x);
                    let mut pd = (x - v.ph2_last[s]).abs();
                    if pd > 0.5 {
                        pd = 1.0 - pd;
                    }
                    let mut o = 0.0;
                    if pd != 0.0 {
                        let mut len = 1.0 / pd;
                        if v.counter2[s] > 0 {
                            len /= v.counter2[s] as f32;
                            v.counter2[s] -= 1;
                        }
                        o = oscillate(w, o2w, x, len, rng);
                    }
                    o *= d.vol[1][s];
                    if o2v_mod {
                        o = mod_vol(&d, T_VOL + 1, o, &env, &lfo);
                    }
                    if v.invert2[s] {
                        o = -o;
                    }
                    o2[s] = o;
                    v.ph2_last[s] = x;
                    ph[1][s] += 1.0 / (rate / f);
                }
                // Oscillator 3.
                for s in 0..2 {
                    let f = if o3f_mod {
                        freq_mod(base[2][s], T_PIT + 2)
                    } else {
                        base[2][s]
                    };
                    let mut x = ph[2][s];
                    if o3p_mod {
                        x += add_mod(&d, T_PHS + 2, &env, &lfo);
                    }
                    if omod == MOD_PM {
                        x += o2[s] * 0.5;
                    }
                    let x = abs_fraction(x);
                    let mut pd = (x - v.ph3_last[s]).abs();
                    if pd > 0.5 {
                        pd = 1.0 - pd;
                    }
                    let (mut a, mut b) = (0.0, 0.0);
                    if pd != 0.0 {
                        let mut len = 1.0 / pd;
                        if v.counter3[s] > 0 {
                            len /= v.counter3[s] as f32;
                            v.counter3[s] -= 1;
                        }
                        a = oscillate(w, o3w1, x, len, rng);
                        b = oscillate(w, o3w2, x, len, rng);
                    }
                    let mut sub = d.sub;
                    if o3s_mod {
                        sub = (sub + add_mod(&d, T_SUB, &env, &lfo)).clamp(0.0, 1.0);
                    }
                    let mut o = a + (b - a) * sub;
                    o *= d.vol[2][s];
                    if o3v_mod {
                        o = mod_vol(&d, T_VOL + 2, o, &env, &lfo);
                    }
                    if omod == MOD_AM {
                        o = (o * (1.0 + o2[s]).max(0.0)).clamp(-MODCLIP, MODCLIP);
                    }
                    if v.invert3[s] {
                        o = -o;
                    }
                    v.ph3_last[s] = x;
                    let mut step = 1.0 / (rate / f);
                    // Frequency modulation done as phase modulation.
                    if omod == MOD_FM {
                        step += o2[s] * d.fm_correction;
                    }
                    ph[2][s] += step;
                    // The "integrator": a very simple filter.
                    let mix = o1[s] + o + if omod == MOD_MIX { o2[s] } else { 0.0 };
                    out[s] = mix + (v.out_last[s] - mix) * d.integrator;
                    v.out_last[s] = mix;
                }
                *l_out += out[0] * gain;
                *r_out += out[1] * gain;
                v.played += 1;
                if v.released {
                    v.since_release += 1;
                }
            }
            v.phase = std::array::from_fn(|o| {
                std::array::from_fn(|s| abs_fraction(ph[o][s] - d.po[o][s]))
            });
            if v.released && v.envelopes_done() && v.since_release >= floor {
                self.alloc.idle(vi);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, pitch, play, sounds_and_falls_silent, tuned_to};
    use super::*;

    fn defaults() -> Vec<f32> {
        params().iter().map(|p| p.default).collect()
    }

    #[test]
    fn monstro_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 0.1);
        sounds_and_falls_silent(ID, 44100, &[], 0.1);
        // Every wave of every oscillator and LFO, every modulation, every route of the matrix
        // at once, with both syncs.
        let mut v = defaults();
        for (k, x) in v.iter_mut().enumerate().take(LEVEL).skip(MATRIX) {
            *x = if k % 2 == 0 { 300.0 } else { -200.0 };
        }
        v[O1_SSR] = 1.0;
        v[O1_SSF] = 1.0;
        v[O2_SYNC_H] = 1.0;
        v[O3_SYNC_R] = 1.0;
        v[ENV + 5] = 200.0;
        for wave in 0..WAVES.len() {
            v[O2_WAVE] = wave as f32;
            v[O3_WAVE1] = wave as f32;
            v[O3_WAVE2] = ((wave + 4) % WAVES.len()) as f32;
            v[LFO] = (wave % LFO_WAVES.len()) as f32;
            v[LFO + LFO_PARAMS] = ((wave + 5) % LFO_WAVES.len()) as f32;
            v[O23_MOD] = (wave % MODS.len()) as f32;
            sounds_and_falls_silent(ID, 48000, &v, 0.5);
        }
    }

    #[test]
    fn the_note_sets_the_pitch() {
        for (note, hz) in [(45u8, 110.0), (57, 220.0), (69, 440.0)] {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 24000, 256, &[(0, 0x90, note, 100)]);
            let got = pitch(&l[4800..], 48000.0, 60.0, 1000.0);
            assert!((got / hz - 1.0).abs() < 0.01, "{note}: {got}");
        }
        // Coarse moves an oscillator by semitones: oscillator 2 alone, an octave up.
        let mut v = defaults();
        v[O1_VOL] = 0.0;
        v[O3_VOL] = 0.0;
        v[O2_CRS] = 12.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &v) else {
            panic!("does not open")
        };
        let (l, _) = play(&mut inst, 24000, 256, &[(0, 0x90, 57, 100)]);
        assert!(tuned_to(&l[4800..], 440.0, 48000.0) > 10.0);
    }

    /// The matrix: envelope 1 on every oscillator's volume makes the attack and the release
    /// heard; envelope 1 on oscillator 1's pitch raises it by two octaves at full amount.
    #[test]
    fn the_matrix_routes_the_envelopes() {
        let mut v = defaults();
        for t in [T_VOL, T_VOL + 1, T_VOL + 2] {
            v[MATRIX + t * 4] = 1000.0;
        }
        v[ENV + 1] = 200.0; // attack
        v[ENV + 5] = 300.0; // release
        let Some(mut inst) = open_native_instrument(ID, 48000, &v) else {
            panic!("does not open")
        };
        let (l, _) = play(
            &mut inst,
            48000,
            256,
            &[(0, 0x90, 57, 127), (24000, 0x80, 57, 0)],
        );
        let (early, full) = (peak(&l[..960]), peak(&l[12000..24000]));
        assert!(early < full * 0.2, "{early} {full}");
        // Still sounding 100 ms after the key, falling.
        assert!(peak(&l[28800..30000]) > 0.05 && peak(&l[28800..30000]) < full);
        // And over after the 300 ms release.
        assert_eq!(peak(&l[39600..]), 0.0);

        let mut v = defaults();
        v[O2_VOL] = 0.0;
        v[O3_VOL] = 0.0;
        v[O1_PW] = 5000.0;
        v[MATRIX + T_PIT * 4] = 1000.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &v) else {
            panic!("does not open")
        };
        let (l, _) = play(&mut inst, 24000, 256, &[(0, 0x90, 45, 100)]);
        let got = pitch(&l[4800..], 48000.0, 60.0, 1000.0);
        assert!((got / 440.0 - 1.0).abs() < 0.01, "{got}");
    }

    /// The four ways oscillator 2 meets oscillator 3 sound different, and an LFO on the pitch
    /// moves it.
    #[test]
    fn the_modulations_differ_and_the_lfo_moves_the_pitch() {
        let render = |set: &[(usize, f32)]| {
            let mut v = defaults();
            for (k, x) in set {
                v[*k] = *x;
            }
            let Some(mut inst) = open_native_instrument(ID, 48000, &v) else {
                panic!("does not open")
            };
            play(&mut inst, 9600, 256, &[(0, 0x90, 57, 100)]).0
        };
        let outs: Vec<Vec<f32>> = (0..4)
            .map(|m| render(&[(O23_MOD, m as f32), (O2_CRS, 7.0), (O2_WAVE, 2.0)]))
            .collect();
        for a in 0..4 {
            for b in a + 1..4 {
                let diff = outs[a]
                    .iter()
                    .zip(&outs[b])
                    .fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
                assert!(diff > 0.05, "{} and {}: {diff}", MODS[a], MODS[b]);
            }
        }
        // LFO 1 at 5 Hz (200 ms) on every pitch: the pitch moves.
        let set = [
            (LFO + 2, 2000.0),
            (MATRIX + T_PIT * 4 + 2, 500.0),
            (MATRIX + (T_PIT + 1) * 4 + 2, 500.0),
            (MATRIX + (T_PIT + 2) * 4 + 2, 500.0),
        ];
        let l = render(&set);
        let a = pitch(&l[0..2400], 48000.0, 60.0, 1000.0);
        let b = pitch(&l[4800..7200], 48000.0, 60.0, 1000.0);
        assert!((a / b - 1.0).abs() > 0.05, "{a} {b}");
    }

    /// Oscillator 1 restarts oscillator 2 at each rise of its pulse when hard sync is on.
    #[test]
    fn hard_sync_follows_oscillator_1() {
        let mut v = defaults();
        v[O3_VOL] = 0.0;
        v[O1_VOL] = 0.0;
        v[O2_CRS] = 5.0;
        v[O2_WAVE] = 11.0;
        v[O1_SSR] = 1.0;
        v[O2_SYNC_H] = 1.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &v) else {
            panic!("does not open")
        };
        let (l, _) = play(&mut inst, 24000, 256, &[(0, 0x90, 45, 100)]);
        // Oscillator 2 now repeats at oscillator 1's period: 110 Hz.
        let got = pitch(&l[4800..], 48000.0, 60.0, 1000.0);
        assert!((got / 110.0 - 1.0).abs() < 0.02, "{got}");
    }

    /// The band-limited tables: normalized, and with no harmonic at or above half the table.
    #[test]
    fn the_wavetables_are_band_limited() {
        let w = waves();
        for wave in 0..4 {
            for (t, table) in w.0[wave].iter().enumerate() {
                assert_eq!(table.len(), TLENS[t]);
                let most = table.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                assert!(most <= 1.0 + 1e-6, "{wave} {t}");
            }
        }
        // The saw of 96 samples: harmonic 40 is there, at 1/40 of the first.
        let saw = &w.0[BL_SAW][11];
        let h = |k: usize| super::super::test_notes::level_at(saw, k as f64, 96.0);
        assert!((h(40) / h(1) - 1.0 / 40.0).abs() < 0.002);
        // Table choice: a wave 100 samples long reads the 96-sample table.
        let (a, b) = (
            w.oscillate(0.3, 100.0, BL_SAW),
            w.oscillate(0.3, 97.0, BL_SAW),
        );
        assert_eq!(a, b);
        // The square's middle: below 1 by its ripple's overshoot, which the normalizing takes
        // out.
        let mid = w.oscillate(0.25, 1000.0, BL_SQUARE);
        assert!(mid > 0.8 && mid < 0.95, "{mid}");
    }

    #[test]
    fn the_helpers_are_the_originals() {
        assert_eq!(fraction(-2.25), -0.25);
        assert_eq!(abs_fraction(-2.25), 0.75);
        // fastPow is close to pow for the slopes the envelopes use.
        for (a, b) in [(0.5f64, 0.1f64), (0.25, 10.0), (0.9, 2.0)] {
            assert!((fast_pow(a, b) - a.powf(b)).abs() < 0.08, "{a} {b}");
        }
        let mut r = FastRand(1);
        assert_eq!(r.next(), 16838);
        assert!(triangle_sample(0.25) == 1.0 && triangle_sample(0.75) == -1.0);
        assert!(exp_sample(0.5) == 1.0 && exp_sample(0.0) == -1.0);
        assert_eq!(moog_saw_sample(0.25), 0.0);
        let d = derive(
            &std::array::from_fn(|k| params()[k].default as i32),
            44100.0,
        );
        assert!((d.integrator - INTEGRATOR).abs() < 1e-6);
        assert_eq!(d.counter_max, 5);
        assert!((d.vol[0][0] - 0.33).abs() < 1e-6);
    }
}
