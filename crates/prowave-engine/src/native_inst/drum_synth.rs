// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The sound is ported from Bespoke Synth's DrumSynth
// (Source/DrumSynth.cpp and .h, with the parts of ADSR.cpp, EnvOscillator.cpp, Oscillator.cpp,
// BiquadFilter.cpp and .h, MathUtils.cpp and SynthGlobals.cpp it uses; Copyright (C) 2021 Ryan
// Challinor; GPL-3.0-or-later; https://github.com/BespokeSynth/BespokeSynth at commit
// 25b1541). Pro Wave's note mapping, parameter table and tests are its own.

//! Drum Synth: eight pads, each a drum made of a tone whose frequency falls and a burst of
//! noise, through a low-pass filter whose cutoff falls.
//!
//! Logic Pro equivalent: Drum Synth (the instrument of that name, which this is not), and
//! Ultrabeat's voices in their simplest form.
//!
//! The pads are the original's eight, numbered as it numbers them (its grid: 1 to 4 along the
//! bottom row, 5 to 8 along the top). The original plays pad `n` on MIDI note `n - 1`, notes 0
//! to 7; here the pads are also on notes 36 to 43 (C1 to G1, the drum notes of a General MIDI
//! kit's bottom octave, where a pad controller's bottom-left pad sends 36): pad 1 is 36, pad 8
//! is 43. A note starts its pad's four envelopes; letting go of the key does nothing (each
//! envelope is an attack and a decay, with no sustain). A pad struck again starts again from
//! where its envelopes are.
//!
//! Each pad: Tone and Noise levels (each heard as its square), the tone's Wave, its frequency
//! falling from Freq Max to Freq Min over the pitch envelope, the noise squared with its sign
//! kept, and, when Cutoff Max is below 10,000 Hz ("None"), a low-pass from Cutoff Max to Cutoff
//! Min over the filter envelope with resonance Q. Velocity sets the tone's and the noise's
//! level, and shortens every attack (to a fifth of it at the lowest velocity). Volume is the
//! whole kit's level (heard as its square), and Oversampling runs the pads at 2, 4 or 8 times
//! the rate and averages each group of samples back down, as the original's setting does.
//! The defaults are the original's: every pad the same, and only pad 1 with any level.
//! The original's separate output for each pad is not ported: the pads are mixed.

use super::NativeInst;
use crate::native_fx::NativeParam;
use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;

pub const ID: &str = "prowave:drum-synth";
pub const NAME: &str = "Drum Synth";

pub const PADS: usize = 8;
pub const VOLUME: usize = 0;
pub const OVERSAMPLING: usize = 1;
/// The first pad's first parameter; pad `p`'s parameter `k` is `PAD + p * PAD_PARAMS + k`.
pub const PAD: usize = 2;
pub const PAD_PARAMS: usize = 16;
pub const COUNT: usize = PAD + PADS * PAD_PARAMS;

/// A pad's parameters, from its first.
pub const TONE: usize = 0;
pub const NOISE: usize = 1;
pub const WAVE: usize = 2;
pub const FREQ_MAX: usize = 3;
pub const FREQ_MIN: usize = 4;
pub const TONE_ATTACK: usize = 5;
pub const TONE_DECAY: usize = 6;
pub const NOISE_ATTACK: usize = 7;
pub const NOISE_DECAY: usize = 8;
pub const PITCH_ATTACK: usize = 9;
pub const PITCH_DECAY: usize = 10;
pub const CUTOFF_MAX: usize = 11;
pub const CUTOFF_MIN: usize = 12;
pub const FILTER_ATTACK: usize = 13;
pub const FILTER_DECAY: usize = 14;
pub const Q: usize = 15;

/// Cutoff Max at this value: no filter (`DRUMSYNTH_NO_CUTOFF`).
pub const NO_CUTOFF: i32 = 10000;
/// The lowest note of the pads.
pub const FIRST_NOTE: u8 = 36;

pub const WAVES: [&str; 4] = ["Sine", "Saw", "Square", "Triangle"];
const OVERSAMPLINGS: [&str; 4] = ["1x", "2x", "4x", "8x"];

const PAD_NAMES: [&str; PAD_PARAMS] = [
    "Tone",
    "Noise",
    "Wave",
    "Freq Max",
    "Freq Min",
    "Tone Attack",
    "Tone Decay",
    "Noise Attack",
    "Noise Decay",
    "Pitch Attack",
    "Pitch Decay",
    "Cutoff Max",
    "Cutoff Min",
    "Filter Attack",
    "Filter Decay",
    "Q",
];

/// The original's ranges and defaults: levels in percent (the original's 0 to 1), times in
/// ms, Q in thousandths, Volume in percent (its 0 to 2).
pub fn params() -> &'static [NativeParam] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    static PARAMS: OnceLock<Vec<NativeParam>> = OnceLock::new();
    PARAMS.get_or_init(|| {
        let names = NAMES.get_or_init(|| {
            (0..PADS * PAD_PARAMS)
                .map(|k| format!("Pad {} {}", k / PAD_PARAMS + 1, PAD_NAMES[k % PAD_PARAMS]))
                .collect()
        });
        let mut out = Vec::with_capacity(COUNT);
        out.push(NativeParam::knob("Volume", 0, 200, 100, "%"));
        out.push(NativeParam::choice("Oversampling", 0, 0, &OVERSAMPLINGS));
        for (k, name) in names.iter().enumerate() {
            let (pad, name) = (k / PAD_PARAMS, name.as_str());
            out.push(match k % PAD_PARAMS {
                // Only the first pad has a level at first.
                TONE => NativeParam::knob(name, 0, 100, if pad == 0 { 50 } else { 0 }, "%"),
                NOISE => NativeParam::knob(name, 0, 100, 0, "%"),
                WAVE => NativeParam::choice(name, 0, 0, &WAVES),
                FREQ_MAX => NativeParam::knob(name, 0, 1600, 150, "Hz"),
                FREQ_MIN => NativeParam::knob(name, 0, 1600, 10, "Hz"),
                TONE_DECAY => NativeParam::knob(name, 1, 500, 100, "ms"),
                NOISE_DECAY => NativeParam::knob(name, 1, 500, 40, "ms"),
                PITCH_DECAY | FILTER_DECAY => NativeParam::knob(name, 1, 500, 500, "ms"),
                CUTOFF_MAX => NativeParam::knob(name, 10, NO_CUTOFF, NO_CUTOFF, "Hz"),
                CUTOFF_MIN => NativeParam::knob(name, 10, NO_CUTOFF, 10, "Hz"),
                Q => NativeParam::knob(name, 100, 20000, 707, ""),
                // The attacks.
                _ => NativeParam::knob(name, 1, 500, 1, "ms"),
            });
        }
        out
    })
}

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        VOLUME => format!("{v}%"),
        OVERSAMPLING => OVERSAMPLINGS
            .get(v.max(0) as usize)
            .copied()
            .unwrap_or("")
            .to_string(),
        k if (PAD..COUNT).contains(&k) => match (k - PAD) % PAD_PARAMS {
            TONE | NOISE => format!("{v}%"),
            WAVE => WAVES
                .get(v.max(0) as usize)
                .copied()
                .unwrap_or("")
                .to_string(),
            FREQ_MAX | FREQ_MIN | CUTOFF_MIN => format!("{v}Hz"),
            CUTOFF_MAX if v >= NO_CUTOFF => "None".to_string(),
            CUTOFF_MAX => format!("{v}Hz"),
            Q => format!("{:.3}", v as f32 / 1000.0),
            _ => format!("{v}ms"),
        },
        _ => String::new(),
    }
}

/// The pad (0 to 7) that `note` plays: notes 0 to 7, as in the original, and 36 to 43.
pub fn pad_of(note: u8) -> Option<usize> {
    let n = usize::from(note);
    if n < PADS {
        Some(n)
    } else {
        n.checked_sub(usize::from(FIRST_NOTE)).filter(|p| *p < PADS)
    }
}

/// Bespoke's `MathUtils::Curve`.
fn curve(t: f32, amount: f32) -> f32 {
    t.powf((-2.0 * amount).exp())
}

/// One of a pad's envelopes: Bespoke's `ADSR` as the DrumSynth sets it up, two stages and no
/// sustain: a straight rise to 1 times the level it was started with, then a curved fall to 0.
#[derive(Clone, Copy)]
struct Env {
    /// Milliseconds since it was started; negative: never started.
    at: f64,
    /// Where the rise starts: the value when it was started.
    from: f32,
    mult: f32,
    /// What the rise's time is multiplied by.
    scale: f32,
}

impl Env {
    const IDLE: Env = Env {
        at: -1.0,
        from: 0.0,
        mult: 1.0,
        scale: 1.0,
    };

    /// The stage at this moment (2: done) and the time it started, in ms.
    fn stage(&self, attack: f32, decay: f32) -> (usize, f64) {
        if self.at < 0.0 {
            return (2, 0.0);
        }
        let times = [f64::from(attack * self.scale), f64::from(decay)];
        let (mut stage, mut start) = (0usize, 0.0f64);
        while stage < 2 && self.at > times[stage] + start {
            start += times[stage];
            stage += 1;
        }
        (stage, start)
    }

    fn value(&self, attack: f32, decay: f32) -> f32 {
        let (stage, start) = self.stage(attack, decay);
        let (from, to, time) = match stage {
            0 => (self.from, self.mult, attack * self.scale),
            1 => (self.mult, 0.0, decay),
            _ => return 0.0,
        };
        let mut lerp = (((self.at - start) / f64::from(time)) as f32).clamp(0.0, 1.0);
        if stage == 1 {
            // The fall's curve, -0.5, turned over because it falls.
            let c = -0.5 * if from < to { 1.0 } else { -1.0 };
            lerp = curve(lerp, c);
        }
        from + (to - from) * lerp
    }

    fn start(&mut self, mult: f32, scale: f32, attack: f32, decay: f32) {
        let from = self.value(attack, decay);
        *self = Env {
            at: 0.0,
            from,
            mult,
            scale,
        };
    }

    fn is_done(&self, attack: f32, decay: f32) -> bool {
        self.stage(attack, decay).0 == 2
    }
}

/// Bespoke's `BiquadFilter` as a low-pass.
#[derive(Clone, Copy)]
struct LowPass {
    f: f64,
    q: f64,
    a0: f64,
    a1: f64,
    a2: f64,
    b1: f64,
    b2: f64,
    z1: f64,
    z2: f64,
}

impl LowPass {
    fn new(rate: f64) -> LowPass {
        let mut f = LowPass {
            f: 1000.0,
            q: std::f64::consts::FRAC_1_SQRT_2,
            a0: 1.0,
            a1: 0.0,
            a2: 0.0,
            b1: 0.0,
            b2: 0.0,
            z1: 0.0,
            z2: 0.0,
        };
        f.update(rate);
        f
    }

    fn set(&mut self, f: f64, q: f64, rate: f64) {
        if self.f != f || self.q != q {
            self.f = f;
            self.q = q;
            self.update(rate);
        }
    }

    fn update(&mut self, rate: f64) {
        if self.f <= 0.0 || self.f.is_nan() || self.q <= 0.0 {
            (self.a0, self.a1, self.a2, self.b1, self.b2) = (1.0, 0.0, 0.0, 0.0, 0.0);
            return;
        }
        let k = (std::f64::consts::PI * (self.f / rate).clamp(0.0, 0.499_999)).tan();
        let norm = 1.0 / (1.0 + k / self.q + k * k);
        self.a0 = k * k * norm;
        self.a1 = 2.0 * self.a0;
        self.a2 = self.a0;
        self.b1 = 2.0 * (k * k - 1.0) * norm;
        self.b2 = (1.0 - k / self.q + k * k) * norm;
    }

    fn filter(&mut self, x: f32) -> f32 {
        let x = f64::from(x);
        let out = x * self.a0 + self.z1;
        self.z1 = x * self.a1 + self.z2 - self.b1 * out;
        self.z2 = x * self.a2 - self.b2 * out;
        if !out.is_finite() {
            (self.z1, self.z2) = (0.0, 0.0);
        }
        out as f32
    }
}

/// Bespoke's `Oscillator::Value` for the four waves a pad offers (pulse width one half, no
/// shuffle, no softening).
fn wave(kind: i32, phase: f32) -> f32 {
    let mut phase = phase;
    if kind == 3 {
        // The triangle shifted to start at zero.
        phase += 0.5 * PI;
    }
    phase %= TAU;
    match kind {
        1 => phase / TAU * 2.0 - 1.0,
        2 => {
            if phase > TAU * 0.5 {
                -1.0
            } else {
                1.0
            }
        }
        3 => (phase / TAU - 0.5).abs() * 4.0 - 1.0,
        _ => phase.sin(),
    }
}

/// One pad: `DrumSynthHit`.
#[derive(Clone, Copy)]
struct Hit {
    tone: Env,
    noise: Env,
    freq: Env,
    filter: Env,
    phase: f32,
    lowpass: LowPass,
}

/// The instrument.
pub struct DrumSynth {
    rate: f64,
    applied: [i32; COUNT],
    hits: [Hit; PADS],
    seed: u32,
}

impl DrumSynth {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let rate = f64::from(rate.max(1));
        let mut applied = [0; COUNT];
        for (k, (a, p)) in applied.iter_mut().zip(params()).enumerate() {
            *a = p.stored(values.get(k).copied().unwrap_or(p.default));
        }
        DrumSynth {
            rate,
            applied,
            hits: [Hit {
                tone: Env::IDLE,
                noise: Env::IDLE,
                freq: Env::IDLE,
                filter: Env::IDLE,
                phase: 0.0,
                lowpass: LowPass::new(rate),
            }; PADS],
            seed: 0x1234_5679,
        }
    }
}

/// (attack, decay) in ms of pad `pad`'s envelope whose attack is pad parameter `attack`.
fn times(applied: &[i32; COUNT], pad: usize, attack: usize) -> (f32, f32) {
    let k = PAD + pad * PAD_PARAMS + attack;
    (applied[k] as f32, applied[k + 1] as f32)
}

impl NativeInst for DrumSynth {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = params().get(index) {
            self.applied[index] = p.stored(value);
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        let Some(p) = pad_of(note) else {
            return;
        };
        let velocity = f32::from(velocity) / 127.0;
        let scale = 0.2 + 0.8 * velocity;
        let a = &self.applied;
        let (tone, noise) = (times(a, p, TONE_ATTACK), times(a, p, NOISE_ATTACK));
        let (freq, filter) = (times(a, p, PITCH_ATTACK), times(a, p, FILTER_ATTACK));
        let h = &mut self.hits[p];
        h.freq.start(1.0, scale, freq.0, freq.1);
        h.filter.start(1.0, scale, filter.0, filter.1);
        h.tone.start(velocity, scale, tone.0, tone.1);
        h.noise.start(velocity, scale, noise.0, noise.1);
    }

    fn note_off(&mut self, _note: u8) {}

    fn all_notes_off(&mut self) {}

    fn all_sound_off(&mut self) {
        for h in &mut self.hits {
            h.tone = Env::IDLE;
            h.noise = Env::IDLE;
            h.phase = 0.0;
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        let os = 1usize << self.applied[OVERSAMPLING].clamp(0, 3);
        let rate = self.rate * os as f64;
        let step_ms = 1000.0 / rate;
        let phase_scale = TAU / self.rate as f32 / os as f32;
        let volume = self.applied[VOLUME] as f32 / 100.0;
        let vol_sq = volume * volume / os as f32;
        let a = &self.applied;
        for (p, h) in self.hits.iter_mut().enumerate() {
            let tone_t = times(a, p, TONE_ATTACK);
            let noise_t = times(a, p, NOISE_ATTACK);
            let freq_t = times(a, p, PITCH_ATTACK);
            let filter_t = times(a, p, FILTER_ATTACK);
            if h.tone.is_done(tone_t.0, tone_t.1) && h.noise.is_done(noise_t.0, noise_t.1) {
                h.phase = 0.0;
                continue;
            }
            let pad = PAD + p * PAD_PARAMS;
            let (fmin, fmax) = (a[pad + FREQ_MIN] as f32, a[pad + FREQ_MAX] as f32);
            let (cmin, cmax) = (a[pad + CUTOFF_MIN] as f32, a[pad + CUTOFF_MAX]);
            let q = f64::from(a[pad + Q]) / 1000.0;
            let filtered = cmax != NO_CUTOFF;
            let vol = a[pad + TONE] as f32 / 100.0;
            let vol_noise = a[pad + NOISE] as f32 / 100.0;
            let kind = a[pad + WAVE];
            for out in left.iter_mut() {
                let mut sum = 0.0f32;
                for _ in 0..os {
                    let freq = fmin + (fmax - fmin) * h.freq.value(freq_t.0, freq_t.1);
                    if filtered {
                        let env = h.filter.value(filter_t.0, filter_t.1);
                        let cutoff = cmin + (cmax as f32 - cmin) * env;
                        h.lowpass.set(f64::from(cutoff), q, rate);
                    }
                    let mut sample =
                        wave(kind, h.phase) * h.tone.value(tone_t.0, tone_t.1) * vol * vol;
                    // The original's random oscillator: -1 to 1.
                    self.seed = self
                        .seed
                        .wrapping_mul(1_664_525)
                        .wrapping_add(1_013_904_223);
                    let rnd = (self.seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
                    let mut noise = rnd * h.noise.value(noise_t.0, noise_t.1);
                    // Squared, keeping the sign.
                    noise *= noise.abs();
                    sample += noise * vol_noise * vol_noise;
                    if filtered {
                        sample = h.lowpass.filter(sample);
                    }
                    sum += sample;
                    h.phase += freq * phase_scale;
                    while h.phase > TAU {
                        h.phase -= TAU;
                    }
                    for e in [&mut h.tone, &mut h.noise, &mut h.freq, &mut h.filter] {
                        if e.at >= 0.0 {
                            e.at += step_ms;
                        }
                    }
                }
                *out += sum * vol_sq;
            }
        }
        right.copy_from_slice(left);
    }
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, play, tuned_to};
    use super::*;

    /// Every pad at a level, and a value for pad `p`'s parameter `k`.
    fn kit(set: &[(usize, usize, f32)]) -> Vec<f32> {
        let mut v: Vec<f32> = params().iter().map(|p| p.default).collect();
        for p in 0..PADS {
            v[PAD + p * PAD_PARAMS + TONE] = 80.0;
        }
        for (p, k, x) in set {
            v[PAD + p * PAD_PARAMS + k] = *x;
        }
        v
    }

    #[test]
    fn drum_synth_does_not_allocate() {
        for rate in [48000u32, 44100] {
            for os in 0..4 {
                let mut values = kit(&[(1, NOISE, 70.0), (2, CUTOFF_MAX, 3000.0)]);
                values[OVERSAMPLING] = os as f32;
                let Some(mut inst) = open_native_instrument(ID, rate, &values) else {
                    panic!("does not open")
                };
                let n = rate as usize;
                let hits: Vec<(usize, u8, u8, u8)> = (0..PADS as u8)
                    .map(|p| (usize::from(p) * 2000, 0x90, FIRST_NOTE + p, 110))
                    .collect();
                let (l, r) = play(&mut inst, n, 256, &hits);
                assert!(l.iter().chain(&r).all(|v| v.is_finite()));
                assert!(peak(&l[..n / 2]) > 0.01, "{rate} {os}: {}", peak(&l));
                // Every envelope is over by half a second after the last hit.
                let (l, _) = play(&mut inst, n / 4, 256, &[]);
                assert_eq!(peak(&l), 0.0, "{rate} {os}");
            }
        }
    }

    /// Notes 36 to 43 and 0 to 7 are the pads, in the original's order; others play nothing.
    #[test]
    fn the_notes_are_the_pads() {
        assert_eq!(pad_of(36), Some(0));
        assert_eq!(pad_of(43), Some(7));
        assert_eq!(pad_of(0), Some(0));
        assert_eq!(pad_of(7), Some(7));
        assert_eq!(pad_of(35), None);
        assert_eq!(pad_of(44), None);
        assert_eq!(pad_of(60), None);
        // Each note sounds its own pad: a pad tuned apart from the others.
        for p in 0..PADS {
            let values = kit(&[(p, FREQ_MAX, 1000.0), (p, FREQ_MIN, 1000.0)]);
            let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                panic!("does not open")
            };
            let note = FIRST_NOTE + p as u8;
            let (l, _) = play(&mut inst, 2400, 256, &[(0, 0x90, note, 127)]);
            assert!(tuned_to(&l[100..2400], 1000.0, 48000.0) > 4.0, "pad {p}");
        }
        // The defaults: only pad 1 is heard.
        let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
            panic!("does not open")
        };
        let (l, _) = play(&mut inst, 4800, 256, &[(0, 0x90, 37, 127)]);
        assert_eq!(peak(&l), 0.0);
        let (l, _) = play(&mut inst, 4800, 256, &[(0, 0x90, 36, 127)]);
        assert!(peak(&l) > 0.1);
    }

    /// The tone's frequency falls from Freq Max to Freq Min over the pitch envelope.
    #[test]
    fn the_pitch_falls_from_max_to_min() {
        let values = kit(&[
            (0, FREQ_MAX, 800.0),
            (0, FREQ_MIN, 200.0),
            (0, PITCH_DECAY, 100.0),
            (0, TONE_DECAY, 500.0),
        ]);
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        let (l, _) = play(&mut inst, 24000, 256, &[(0, 0x90, 36, 127)]);
        let crossings = |a: usize, b: usize| {
            l[a..b]
                .windows(2)
                .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
                .count()
        };
        // After the 101 ms of the envelope: 200 Hz, 20 cycles in 100 ms.
        let late = crossings(9600, 14400);
        assert!((19..=21).contains(&late), "{late}");
        // In the first 10 ms it is well above 200 Hz (two cycles).
        let early = crossings(0, 480);
        assert!(early >= 4, "{early}");
    }

    /// Bespoke's envelope: a straight rise over the attack, then the curved fall, and done.
    #[test]
    fn the_envelope_is_the_originals() {
        let mut e = Env::IDLE;
        assert!(e.is_done(1.0, 100.0));
        e.start(0.5, 1.0, 10.0, 100.0);
        e.at = 5.0;
        assert!((e.value(10.0, 100.0) - 0.25).abs() < 1e-6);
        e.at = 60.0;
        // Half-way down the fall: 0.5 * (1 - 0.5^e^-1).
        let want = 0.5 * (1.0 - 0.5f32.powf((-1.0f32).exp()));
        assert!((e.value(10.0, 100.0) - want).abs() < 1e-5);
        e.at = 111.0;
        assert!(e.is_done(10.0, 100.0) && e.value(10.0, 100.0) == 0.0);
        // A low velocity shortens the attack to a fifth.
        e.start(1.0, 0.2, 10.0, 100.0);
        e.at = 2.5;
        assert!(e.stage(10.0, 100.0).0 == 1);
        // Struck again while sounding: the rise starts from where it is.
        let mut e = Env::IDLE;
        e.start(1.0, 1.0, 1.0, 100.0);
        e.at = 50.0;
        let now = e.value(1.0, 100.0);
        e.start(1.0, 1.0, 1.0, 100.0);
        assert_eq!(e.value(1.0, 100.0), now);
    }

    #[test]
    fn the_waves_and_the_filter_are_the_originals() {
        assert!(wave(0, PI / 2.0) > 0.999);
        assert_eq!(wave(1, 0.0), -1.0);
        assert_eq!(wave(2, 1.0), 1.0);
        assert_eq!(wave(2, 4.0), -1.0);
        // The triangle starts at zero.
        assert!(wave(3, 0.0).abs() < 1e-6);
        // The low-pass passes DC and stops what is far above its cutoff.
        let mut f = LowPass::new(48000.0);
        f.set(200.0, std::f64::consts::FRAC_1_SQRT_2, 48000.0);
        let mut last = 0.0;
        for _ in 0..48000 {
            last = f.filter(1.0);
        }
        assert!((last - 1.0).abs() < 1e-3);
        let mut most = 0.0f32;
        for i in 0..4800 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            let y = f.filter(x);
            if i > 2400 {
                most = most.max(y.abs());
            }
        }
        assert!(most < 1e-3, "{most}");
    }
}
