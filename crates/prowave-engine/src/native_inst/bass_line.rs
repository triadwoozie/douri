// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The sound is ported from LMMS's LB302
// (plugins/Lb302/Lb302.cpp and Lb302.h; Copyright (c) 2006-2008 Paul Giblock, Copyright (c)
// 2026 Fawn Sannar; its two-pole filter is based on the gsyn filter code by Andy Sloane, its
// three-pole filter on the TB-303 instrument by Josep M Comajuncosas for the CSounds library;
// GPL-2.0-or-later, taken here under GPL-3.0-or-later; https://github.com/LMMS/lmms at commit
// f085f1e). Pro Wave's key handling, parameter table and tests are its own.

//! Bass Line: a one-voice bass synthesizer in the manner of a 1980s bass-line box: one
//! oscillator into a resonant filter whose cutoff an envelope sweeps down on each note, with
//! slides between notes.
//!
//! Logic Pro equivalent: none directly; ES M and Retro Synth cover the ground.
//!
//! One oscillator (eight waveforms) through one of two filters (a two-pole resonator with a
//! distortion, or a three-pole ladder for 24 dB), then a level that rises quickly on a note
//! and falls when the last key goes up. The filter's envelope is restarted by each note and
//! recalculated every 64 samples, as in the original. With Slide on, each note glides from
//! the one before; with Dead on, a note played while another sounds does not restart the
//! level or the filter. The original's four band-limited waveforms read LMMS's wavetable files
//! and are not here. The newest key held plays; velocity is the note's level.

use super::{stored, NativeInst};
use crate::native_fx::NativeParam;

pub const ID: &str = "prowave:bass-line";
pub const NAME: &str = "Bass Line";

pub const CUTOFF: usize = 0;
pub const RESONANCE: usize = 1;
pub const ENV_MOD: usize = 2;
pub const DECAY: usize = 3;
pub const DISTORTION: usize = 4;
pub const WAVEFORM: usize = 5;
pub const SLIDE_DECAY: usize = 6;
pub const SLIDE: usize = 7;
pub const DEAD: usize = 8;
pub const DB24: usize = 9;
pub const TUNE: usize = 10;
pub const LEVEL: usize = 11;
pub const COUNT: usize = 12;

static WAVES: [&str; 8] = [
    "Sawtooth",
    "Triangle",
    "Square",
    "Round Square",
    "Moog",
    "Sine",
    "Exponential",
    "White Noise",
];
static OFF_ON: [&str; 2] = ["Off", "On"];

/// The original's ranges and defaults, in hundredths.
pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::knob("Cutoff", 0, 150, 75, ""),
    NativeParam::knob("Resonance", 0, 125, 75, ""),
    NativeParam::knob("Env Mod", 0, 100, 10, ""),
    NativeParam::knob("Decay", 0, 100, 10, ""),
    NativeParam::knob("Distortion", 0, 100, 0, ""),
    NativeParam::choice("Waveform", 0, 0, &WAVES),
    NativeParam::knob("Slide Decay", 0, 100, 60, ""),
    NativeParam::choice("Slide", 0, 0, &OFF_ON),
    NativeParam::choice("Dead", 0, 0, &OFF_ON),
    NativeParam::choice("24 dB Filter", 0, 0, &OFF_ON),
    NativeParam::knob("Tune", -24, 24, 0, "st"),
    NativeParam::knob("Level", -48, 12, -6, "dB"),
];

pub fn value_text(k: usize, v: i32) -> String {
    let Some(p) = PARAMS.get(k) else {
        return String::new();
    };
    if let Some(name) = p.choices.get(v.max(0) as usize) {
        return (*name).to_string();
    }
    match k {
        TUNE => format!("{v}st"),
        LEVEL => format!("{v}dB"),
        _ => format!("{:.2}", v as f32 / 100.0),
    }
}

/// Samples between two recalculations of the filter's envelope.
const ENV_INC: u32 = 64;
const VCA_ATTACK: f32 = 1.0 - 0.964_060_9;
const VCA_INITIAL: f32 = 0.5;
/// A level below this is silence.
const GATE: f32 = 1.0 / 65536.0;
/// Seconds the level takes to fall to `GATE`.
const VCA_DECAY_SECONDS: f32 = 0.245_260_77;
const DIST_RATIO: f32 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Vca {
    Attack,
    Decay,
    Idle,
    NeverPlayed,
}

/// What the knobs give both filters (`Lb302FilterKnobState`).
#[derive(Clone, Copy, Default)]
struct Knobs {
    cutoff: f32,
    reso: f32,
    envmod: f32,
    envdecay: f32,
    dist: f32,
}

/// The envelope part both filters share, and each one's own state.
#[derive(Clone, Copy, Default)]
struct Filters {
    // The two-pole filter (`Lb302FilterIIR2`).
    c0: f32,
    e0: f32,
    e1: f32,
    res_coeff: f32,
    d: [f32; 2],
    a: f32,
    b: f32,
    c: f32,
    dist_threshold: f32,
    // The three-pole filter (`Lb302Filter3Pole`): its own envelope and state.
    c0_3: f32,
    kp: f32,
    kp1h: f32,
    kres: f32,
    ay: [f32; 3],
    lastin: f32,
    value: f32,
}

impl Filters {
    fn recalc(&mut self, fs: &Knobs, rate: f32) {
        self.e1 = (6.109 + 1.5876 * fs.envmod + 2.1553 * fs.cutoff - 1.2 * (1.0 - fs.reso)).exp();
        self.e0 = (5.613 - 0.8 * fs.envmod + 2.1553 * fs.cutoff - 0.7696 * (1.0 - fs.reso)).exp();
        let pi_sr = std::f32::consts::PI / rate;
        self.e0 *= pi_sr;
        self.e1 *= pi_sr;
        self.e1 -= self.e0;
        self.res_coeff = (-1.20 + 3.455 * fs.reso).exp();
        self.dist_threshold = fs.dist * 75.0;
    }

    fn play_note(&mut self) {
        self.c0 = self.e1;
        // The three-pole filter's envelope runs from 1 (its `e[1]`).
        self.c0_3 = 1.0;
    }

    fn env_recalc(&mut self, fs: &Knobs, rate: f32) {
        self.c0 *= fs.envdecay;
        let w = self.e0 + self.c0;
        let k = (-w / self.res_coeff.max(1e-9)).exp();
        self.a = 2.0 * (2.0 * w).cos() * k;
        self.b = -k * k;
        self.c = 1.0 - self.a - self.b;

        self.c0_3 *= fs.envdecay;
        let w = 0.000_001 + self.c0_3;
        let k = fs.cutoff.min(0.975);
        // The original's fixed 44.1 kHz, so the knobs mean the same at every rate.
        let half = 44100.0 / 2.0;
        let kfco = 50.0
            + k * ((2300.0 - 1600.0 * fs.envmod)
                + w * (700.0 + 1500.0 * k + (1500.0 + k * (half - 6000.0)) * fs.envmod));
        let kfcn = (2.0 * kfco / rate).min(0.99);
        self.kp = ((-2.7528 * kfcn + 3.0429) * kfcn + 1.718) * kfcn - 0.9984;
        let kp1 = self.kp + 1.0;
        self.kp1h = 0.5 * kp1;
        self.kres = fs.reso * (((-2.7079 * kp1 + 10.963) * kp1 - 14.934) * kp1 + 8.4974);
        self.value = 1.0 + (fs.dist * (1.5 + 2.0 * self.kres * (1.0 - kfcn)));
    }

    #[inline]
    fn two_pole(&mut self, x: f32, fs: &Knobs) -> f32 {
        let mut ret = self.a * self.d[0] + self.b * self.d[1] + self.c * x;
        self.d[1] = self.d[0];
        self.d[0] = ret;
        if fs.dist > 0.0 {
            // LMMS's `DspEffectLibrary::Distortion`, at gain 1.
            let (t, a) = (self.dist_threshold, ret.abs());
            ret = ret * (a + t) / (ret * ret + (t - 1.0) * a + 1.0);
        }
        ret
    }

    #[inline]
    fn three_pole(&mut self, x: f32, fs: &Knobs) -> f32 {
        let ax1 = self.lastin;
        let (ay1, ay2) = (self.ay[0], self.ay[1]);
        self.lastin = x - (self.kres * self.ay[2]).tanh();
        self.ay[0] = self.kp1h * (self.lastin + ax1) - self.kp * self.ay[0];
        self.ay[1] = self.kp1h * (self.ay[0] + ay1) - self.kp * self.ay[1];
        self.ay[2] = self.kp1h * (self.ay[1] + ay2) - self.kp * self.ay[2];
        (self.ay[2] * self.value).tanh() * 3.0 / (1.0 + fs.dist)
    }

    /// A filter that went wrong starts again from rest.
    fn recover(&mut self) {
        if !(self.d[0].is_finite() && self.ay[2].is_finite() && self.lastin.is_finite()) {
            self.d = [0.0; 2];
            self.ay = [0.0; 3];
            self.lastin = 0.0;
        }
    }
}

/// Keys that can be down at once and remembered.
const MAX_HELD: usize = 16;

/// The instrument.
pub struct BassLine {
    rate: f32,
    applied: [i32; COUNT],
    fs: Knobs,
    filters: Filters,
    /// The filter must be recalculated (a knob moved).
    dirty: bool,
    vco_inc: f32,
    vco_c: f32,
    vca: f32,
    vca_mode: Vca,
    env_pos: u32,
    slide: f32,
    slide_inc: f32,
    slide_base: f32,
    held: [u8; MAX_HELD],
    held_len: usize,
    velocity: f32,
    bend: f32,
    seed: u32,
}

impl BassLine {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        BassLine {
            rate: rate.max(1) as f32,
            applied: stored(&PARAMS, values),
            fs: Knobs::default(),
            filters: Filters::default(),
            dirty: true,
            vco_inc: 0.0,
            vco_c: 0.0,
            vca: 0.0,
            vca_mode: Vca::NeverPlayed,
            env_pos: ENV_INC,
            slide: 0.0,
            slide_inc: 0.0,
            slide_base: 0.0,
            held: [0; MAX_HELD],
            held_len: 0,
            velocity: 1.0,
            bend: 0.0,
            seed: 0x1234_5679,
        }
    }

    fn unit(&self, k: usize) -> f32 {
        self.applied[k] as f32 / 100.0
    }

    fn inc_of(&self, note: u8) -> f32 {
        let pitch = f32::from(note) + self.applied[TUNE] as f32 + self.bend;
        440.0 * ((pitch - 69.0) / 12.0).exp2() / self.rate
    }

    /// The knobs into the filters (`filterChanged`, `decayChanged`, `recalcFilter`).
    fn recalc(&mut self) {
        self.fs.cutoff = self.unit(CUTOFF);
        self.fs.reso = self.unit(RESONANCE);
        self.fs.envmod = self.unit(ENV_MOD);
        self.fs.dist = self.unit(DISTORTION) * DIST_RATIO;
        let d = (2.3 * self.unit(DECAY) + 0.2) * self.rate;
        self.fs.envdecay = 0.1f32.powf(1.0 / d * ENV_INC as f32);
        self.filters.recalc(&self.fs, self.rate);
        self.env_pos = ENV_INC;
    }

    /// A new note takes the voice (the `m_newFreq` part of `process`).
    fn start(&mut self, note: u8) {
        let dead = self.applied[DEAD] != 0;
        self.vco_inc = self.inc_of(note);
        self.vca_mode = if !dead || matches!(self.vca_mode, Vca::Decay | Vca::NeverPlayed) {
            Vca::Attack
        } else {
            Vca::Idle
        };
        if self.slide_inc != 0.0 {
            self.slide = self.vco_inc - self.slide_inc;
            self.slide_base = self.vco_inc;
            self.slide_inc = 0.0;
        } else {
            self.slide = 0.0;
        }
        if self.applied[SLIDE] != 0 {
            self.slide_inc = self.vco_inc;
        }
        self.recalc();
        if !dead {
            self.filters.play_note();
        }
    }

    /// The pitch of the note sounding, again (the wheel or Tune moved).
    fn retune(&mut self) {
        if self.held_len == 0 {
            return;
        }
        let inc = self.inc_of(self.held[self.held_len - 1]);
        if self.slide != 0.0 {
            self.slide_base = inc;
        } else {
            self.vco_inc = inc;
        }
    }
}

impl NativeInst for BassLine {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = PARAMS.get(index) {
            let v = p.stored(value);
            if self.applied[index] != v {
                self.applied[index] = v;
                self.dirty = true;
                if index == TUNE {
                    self.retune();
                }
            }
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if let Some(i) = self.held[..self.held_len].iter().position(|n| *n == note) {
            self.held.copy_within(i + 1..self.held_len, i);
            self.held_len -= 1;
        }
        if self.held_len == MAX_HELD {
            self.held.copy_within(1..MAX_HELD, 0);
            self.held_len -= 1;
        }
        self.held[self.held_len] = note;
        self.held_len += 1;
        self.velocity = f32::from(velocity) / 127.0;
        self.start(note);
    }

    fn note_off(&mut self, note: u8) {
        let Some(i) = self.held[..self.held_len].iter().position(|n| *n == note) else {
            return;
        };
        let was_playing = i + 1 == self.held_len;
        self.held.copy_within(i + 1..self.held_len, i);
        self.held_len -= 1;
        if self.held_len == 0 {
            self.vca_mode = Vca::Decay;
        } else if was_playing {
            // Back to the key still down, without a new attack.
            self.retune();
        }
    }

    fn all_notes_off(&mut self) {
        self.held_len = 0;
        if self.vca_mode != Vca::NeverPlayed {
            self.vca_mode = Vca::Decay;
        }
    }

    fn all_sound_off(&mut self) {
        self.held_len = 0;
        self.vca = 0.0;
        self.vca_mode = Vca::NeverPlayed;
        self.slide = 0.0;
        self.slide_inc = 0.0;
        self.filters.d = [0.0; 2];
        self.filters.ay = [0.0; 3];
        self.filters.lastin = 0.0;
    }

    fn bend(&mut self, semitones: f32) {
        if self.bend != semitones {
            self.bend = semitones;
            self.retune();
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        if std::mem::take(&mut self.dirty) {
            self.recalc();
        }
        if self.vca_mode == Vca::NeverPlayed {
            left.fill(0.0);
            right.fill(0.0);
            return;
        }
        let decay = GATE.powf(1.0 / (VCA_DECAY_SECONDS * self.rate));
        let sample_ratio = 44100.0 / self.rate;
        let slide_decay = self.unit(SLIDE_DECAY);
        let wave = self.applied[WAVEFORM];
        let db24 = self.applied[DB24] != 0;
        let gain = 10f32.powf(self.applied[LEVEL] as f32 / 20.0) * self.velocity;
        let fs = self.fs;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.env_pos >= ENV_INC {
                self.filters.env_recalc(&fs, self.rate);
                self.env_pos = 0;
                if self.slide != 0.0 {
                    self.vco_inc = self.slide_base - self.slide;
                    self.slide -= self.slide * (0.1 - slide_decay * 0.0999) * sample_ratio;
                }
            }
            self.env_pos += 1;
            self.vco_c += self.vco_inc;
            if self.vco_c > 0.5 {
                self.vco_c -= 1.0;
            }
            let c = self.vco_c;
            let k = match wave {
                0 => c,
                1 => {
                    let k = c * 2.0 + 0.5;
                    if k > 0.5 {
                        1.0 - k
                    } else {
                        k
                    }
                }
                2 => {
                    if c < 0.0 {
                        0.5
                    } else {
                        -0.5
                    }
                }
                3 => {
                    if c < 0.0 {
                        (1.0 - c * c * 4.0).max(0.0).sqrt() - 0.5
                    } else {
                        -0.5
                    }
                }
                4 => {
                    let mut k = c * 2.0 + 0.5;
                    if k > 1.0 {
                        k = -0.5;
                    } else if k > 0.5 {
                        let w = 2.0 * (k - 0.5) - 1.0;
                        k = 0.5 - (1.0 - w * w).max(0.0).sqrt();
                    }
                    k * 2.0
                }
                5 => 0.5 * (c * std::f32::consts::TAU).sin(),
                6 => {
                    // LMMS's `Oscillator::expSample`.
                    let mut ph = c - c.floor();
                    if ph > 0.5 {
                        ph = 1.0 - ph;
                    }
                    0.5 * (-1.0 + 8.0 * ph * ph)
                }
                _ => {
                    self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    0.5 * ((self.seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0)
                }
            };
            let filtered = if db24 {
                self.filters.three_pole(k, &fs)
            } else {
                self.filters.two_pole(k, &fs)
            };
            let y = filtered * self.vca * gain;
            *l = y;
            *r = y;
            match self.vca_mode {
                Vca::Attack => self.vca += (VCA_INITIAL - self.vca) * VCA_ATTACK,
                Vca::Decay => {
                    self.vca *= decay;
                    if self.vca < GATE {
                        self.vca = 0.0;
                        self.vca_mode = Vca::NeverPlayed;
                    }
                }
                Vca::Idle | Vca::NeverPlayed => {}
            }
        }
        self.filters.recover();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_notes::{peak, pitch, play, sounds_and_falls_silent};
    use super::super::{open_native_instrument, BlockProcessor};
    use super::*;

    #[test]
    fn bass_line_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 1.0);
        sounds_and_falls_silent(ID, 44100, &[], 1.0);
    }

    #[test]
    fn the_note_sets_the_pitch_through_both_filters() {
        for db24 in [0.0f32, 1.0] {
            let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
            values[DB24] = db24;
            // Open and not ringing, so the oscillator is what is measured.
            values[CUTOFF] = 150.0;
            values[RESONANCE] = 20.0;
            for (note, want) in [(33u8, 55.0f64), (45, 110.0), (57, 220.0)] {
                let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                    panic!("does not open")
                };
                let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 100)]);
                assert!(peak(&l) > 0.02, "{}", peak(&l));
                let got = pitch(&l[24000..], 48000.0, 30.0, 1000.0);
                assert!(
                    (got / want - 1.0).abs() < 0.01,
                    "24 dB {db24}: {note} is {got} Hz"
                );
            }
        }
    }

    /// With Slide on the second note starts at the first one's pitch and glides to its own;
    /// with it off it is there at once.
    #[test]
    fn slide_glides_from_the_note_before() {
        let run = |slide: f32| {
            let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
            values[SLIDE] = slide;
            values[CUTOFF] = 150.0;
            values[RESONANCE] = 20.0;
            let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                panic!("does not open")
            };
            let midi = [
                (0, 0x90, 45, 100),
                (24000, 0x90, 57, 100),
                (24010, 0x80, 45, 0),
            ];
            let (l, _) = play(&mut inst, 72000, 256, &midi);
            // Cycles in the 40 ms after the second note, and the pitch long after.
            let cycles = l[24000..25920]
                .windows(2)
                .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
                .count();
            (cycles, pitch(&l[60000..], 48000.0, 60.0, 600.0))
        };
        let (at_once, settled) = run(0.0);
        let (gliding, arrived) = run(1.0);
        // 220 Hz for 40 ms is 8.8 cycles; on the way up from 110 Hz it is fewer.
        assert!(
            (8..=9).contains(&at_once) && gliding < at_once,
            "{at_once} {gliding}"
        );
        assert!((settled / 220.0 - 1.0).abs() < 0.02, "{settled}");
        assert!((arrived / 220.0 - 1.0).abs() < 0.02, "{arrived}");
    }

    /// The 24 dB filter's envelope makes the start of a note brighter than its sustain, more
    /// so with more Env Mod; the level falls away after the key goes up.
    #[test]
    fn the_filter_envelope_and_the_release() {
        let bright = |env_mod: f32| {
            let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
            values[DB24] = 1.0;
            values[ENV_MOD] = env_mod;
            values[CUTOFF] = 30.0;
            values[RESONANCE] = 20.0;
            values[DECAY] = 30.0;
            let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 96000, 256, &[(0, 0x90, 33, 100)]);
            // The energy of the sample-to-sample change over the energy: higher for a
            // brighter sound.
            let bright = |x: &[f32]| {
                x.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>()
                    / x.iter().map(|v| v * v).sum::<f32>().max(1e-12)
            };
            bright(&l[1000..5000]) / bright(&l[90000..94000])
        };
        let (most, none) = (bright(100.0), bright(0.0));
        assert!(most > 1.3 && most > none * 1.2, "{most} {none}");
        let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
            panic!("does not open")
        };
        let (l, _) = play(
            &mut inst,
            48000,
            256,
            &[(0, 0x90, 45, 100), (12000, 0x80, 45, 0)],
        );
        assert!(peak(&l[6000..12000]) > 0.02);
        // A quarter of a second after the key: gone.
        assert_eq!(peak(&l[25000..]), 0.0);
        inst.set_parameter(LEVEL, 0.0);
    }
}
