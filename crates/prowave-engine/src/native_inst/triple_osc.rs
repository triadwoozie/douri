// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The oscillators and the ways they modulate each
// other are ported from LMMS's TripleOscillator (plugins/TripleOscillator/TripleOscillator.cpp;
// Copyright (c) 2004-2014 Tobias Doerffel) and its Oscillator (src/core/Oscillator.cpp and
// include/Oscillator.h; Copyright (c) 2004-2009 Tobias Doerffel, 2018 Dave French);
// GPL-2.0-or-later, taken here under GPL-3.0-or-later; https://github.com/LMMS/lmms at commit
// f085f1e. Pro Wave's voices, envelope, parameter table and tests are its own.

//! Triple Oscillator: three oscillators, each of which can be mixed with the next, or
//! modulate its phase, amplitude or frequency, or be synchronized by it.
//!
//! Logic Pro equivalent: ES2 (three oscillators with FM, ring modulation and sync).
//!
//! Oscillator 3 feeds oscillator 2 by Modulation 2, and the result feeds oscillator 1 by
//! Modulation 1. Each oscillator has a waveform, a volume and panning, a coarse detuning in
//! semitones, a fine detuning for the left and the right channel (the two channels are
//! separate oscillators), a phase offset and a further offset for the left channel. The
//! waveforms are the original's plain ones (its band-limited tables and user waveforms are not
//! here). The original takes its envelope from LMMS's instrument track; here each voice has
//! Pro Wave's ([`Adsr`]).

use super::envelope::{Adsr, AdsrRates, AdsrTimes};
use super::voices::Voices;
use super::NativeInst;
use crate::native_fx::NativeParam;
use std::sync::OnceLock;

pub const ID: &str = "prowave:triple-osc";
pub const NAME: &str = "Triple Oscillator";

/// Oscillator `n` (0 to 2) starts at `n * OSC_PARAMS`: Volume, Pan, Coarse, Fine Left, Fine
/// Right, Phase, Stereo Phase, Wave.
pub const OSC_PARAMS: usize = 8;
pub const MOD_1: usize = 24;
pub const MOD_2: usize = 25;
pub const ATTACK: usize = 26;
pub const DECAY: usize = 27;
pub const SUSTAIN: usize = 28;
pub const RELEASE: usize = 29;
pub const VOICES: usize = 30;
pub const LEVEL: usize = 31;
pub const COUNT: usize = 32;

static WAVES: [&str; 7] = [
    "Sine",
    "Triangle",
    "Saw",
    "Square",
    "Moog Saw",
    "Exponential",
    "White Noise",
];
static MODS: [&str; 5] = ["Phase", "Amplitude", "Mix", "Sync", "Frequency"];
/// The original's default: the oscillators are mixed.
const MIX: i32 = 2;

/// Most voices at once.
pub const MAX_VOICES: usize = 8;

pub fn params() -> &'static [NativeParam] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    static PARAMS: OnceLock<Vec<NativeParam>> = OnceLock::new();
    PARAMS.get_or_init(|| {
        let table = [
            "Volume",
            "Pan",
            "Coarse",
            "Fine Left",
            "Fine Right",
            "Phase",
            "Stereo Phase",
            "Wave",
        ];
        let names = NAMES.get_or_init(|| {
            (0..3 * OSC_PARAMS)
                .map(|k| format!("Osc {} {}", k / OSC_PARAMS + 1, table[k % OSC_PARAMS]))
                .collect()
        });
        let mut out = Vec::with_capacity(COUNT);
        for (k, name) in names.iter().enumerate() {
            let (osc, name) = ((k / OSC_PARAMS) as i32, name.as_str());
            out.push(match k % OSC_PARAMS {
                0 => NativeParam::knob(name, 0, 200, 33, "%"),
                1 => NativeParam::knob(name, -100, 100, 0, ""),
                // The original's defaults: each oscillator an octave under the one before.
                2 => NativeParam::knob(name, -24, 24, -12 * osc, "st"),
                3 | 4 => NativeParam::knob(name, -100, 100, 0, "ct"),
                5 | 6 => NativeParam::knob(name, 0, 360, 0, "°"),
                _ => NativeParam::choice(name, 0, 0, &WAVES),
            });
        }
        out.push(NativeParam::choice("Modulation 1", 0, MIX, &MODS));
        out.push(NativeParam::choice("Modulation 2", 0, MIX, &MODS));
        out.push(NativeParam::knob("Attack", 0, 2000, 3, "ms"));
        out.push(NativeParam::knob("Decay", 0, 5000, 400, "ms"));
        out.push(NativeParam::knob("Sustain", 0, 100, 70, "%"));
        out.push(NativeParam::knob("Release", 0, 5000, 200, "ms"));
        out.push(NativeParam::knob(
            "Voices",
            1,
            MAX_VOICES as i32,
            MAX_VOICES as i32,
            "",
        ));
        out.push(NativeParam::knob("Level", -48, 12, -6, "dB"));
        out
    })
}

pub fn value_text(k: usize, v: i32) -> String {
    let Some(p) = params().get(k) else {
        return String::new();
    };
    if let Some(name) = p.choices.get(v.max(0) as usize) {
        return (*name).to_string();
    }
    match k {
        VOICES => v.to_string(),
        _ if p.unit.is_empty() => v.to_string(),
        _ => format!("{v}{}", p.unit),
    }
}

/// One of the original's plain waveforms at `phase` (in cycles).
#[inline]
fn wave(shape: i32, phase: f32, seed: &mut u32) -> f32 {
    let ph = phase - phase.floor();
    match shape {
        0 => (ph * std::f32::consts::TAU).sin(),
        1 => {
            if ph <= 0.25 {
                ph * 4.0
            } else if ph <= 0.75 {
                2.0 - ph * 4.0
            } else {
                ph * 4.0 - 4.0
            }
        }
        2 => -1.0 + ph * 2.0,
        3 => {
            if ph > 0.5 {
                -1.0
            } else {
                1.0
            }
        }
        4 => {
            if ph < 0.5 {
                -1.0 + ph * 4.0
            } else {
                1.0 - 2.0 * ph
            }
        }
        5 => {
            let ph = if ph > 0.5 { 1.0 - ph } else { ph };
            -1.0 + 8.0 * ph * ph
        }
        _ => {
            *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (*seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
        }
    }
}

/// What the parameters make of one oscillator, for one channel.
#[derive(Clone, Copy, Default)]
struct Osc {
    shape: i32,
    /// Its frequency over the note's, per sample: 2^(cents / 1200) / rate.
    detune: f32,
    phase_offset: f32,
    volume: f32,
}

/// The three oscillators of both channels, from the parameters (`OscillatorObject`'s
/// `updateVolume`, `updateDetuning*` and `updatePhaseOffset*`).
fn oscillators(p: &[i32; COUNT], rate: f32) -> [[Osc; 3]; 2] {
    let mut out = [[Osc::default(); 3]; 2];
    for n in 0..3 {
        let o = &p[n * OSC_PARAMS..(n + 1) * OSC_PARAMS];
        let (volume, pan) = (o[0] as f32 / 100.0, o[1] as f32 / 100.0);
        let (left, right) = if pan >= 0.0 {
            ((1.0 - pan) * volume, volume)
        } else {
            (volume, (1.0 + pan) * volume)
        };
        for (side, fine, extra, vol) in [(0, o[3], o[6], left), (1, o[4], 0, right)] {
            out[side][n] = Osc {
                shape: o[7],
                detune: ((o[2] as f32 * 100.0 + fine as f32) / 1200.0).exp2() / rate,
                phase_offset: (o[5] + extra) as f32 / 360.0,
                volume: vol,
            };
        }
    }
    out
}

struct Voice {
    env: Adsr,
    hz: f32,
    velocity: f32,
    /// Each channel's three phases, in cycles.
    phase: [[f32; 3]; 2],
}

/// The instrument.
pub struct TripleOsc {
    rate: f32,
    applied: [i32; COUNT],
    voices: [Voice; MAX_VOICES],
    alloc: Voices<MAX_VOICES>,
    bend: f32,
    seed: u32,
}

impl TripleOsc {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let mut applied = [0i32; COUNT];
        for (k, p) in params().iter().enumerate() {
            applied[k] = p.stored(values.get(k).copied().unwrap_or(p.default));
        }
        TripleOsc {
            rate: rate.max(1) as f32,
            applied,
            voices: std::array::from_fn(|_| Voice {
                env: Adsr::new(),
                hz: 440.0,
                velocity: 0.0,
                phase: [[0.0; 3]; 2],
            }),
            alloc: Voices::new(),
            bend: 0.0,
            seed: 0x9e37_79b9,
        }
    }
}

impl NativeInst for TripleOsc {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = params().get(index) {
            self.applied[index] = p.stored(value);
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        let limit = (self.applied[VOICES].max(1) as usize).min(MAX_VOICES);
        let i = self.alloc.start(note, limit);
        let oscs = oscillators(&self.applied, self.rate);
        let v = &mut self.voices[i];
        v.hz = 440.0 * ((f32::from(note) - 69.0) / 12.0).exp2();
        v.velocity = f32::from(velocity) / 127.0;
        // Each oscillator starts at its phase offset.
        for (phases, side) in v.phase.iter_mut().zip(&oscs) {
            for (phase, osc) in phases.iter_mut().zip(side) {
                *phase = osc.phase_offset;
            }
        }
        v.env.gate_on();
    }

    fn note_off(&mut self, note: u8) {
        if let Some(i) = self.alloc.stop(note) {
            self.voices[i].env.gate_off();
        }
    }

    fn all_notes_off(&mut self) {
        let voices = &mut self.voices;
        self.alloc.stop_all(|i| voices[i].env.gate_off());
    }

    fn all_sound_off(&mut self) {
        self.alloc.clear();
        for v in &mut self.voices {
            v.env.kill();
        }
    }

    fn bend(&mut self, semitones: f32) {
        self.bend = semitones;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        let p = self.applied;
        let oscs = oscillators(&p, self.rate);
        let rates = AdsrRates::new(
            &AdsrTimes {
                attack_s: p[ATTACK] as f32 * 0.001,
                decay_s: p[DECAY] as f32 * 0.001,
                sustain: p[SUSTAIN] as f32 / 100.0,
                release_s: p[RELEASE] as f32 * 0.001,
            },
            self.rate,
        );
        let gain = 0.5 * 10f32.powf(p[LEVEL] as f32 / 20.0);
        let bend = (self.bend / 12.0).exp2();
        // The original's frequency modulation is scaled to mean the same at every rate.
        let fm_scale = 44100.0 / self.rate;
        let (mod_1, mod_2) = (p[MOD_1], p[MOD_2]);
        let seed = &mut self.seed;
        for (i, v) in self.voices.iter_mut().enumerate() {
            if !self.alloc.is_busy(i) {
                continue;
            }
            let hz = v.hz * bend;
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                let level = rates.tick(&mut v.env) * v.velocity * gain;
                let mut out = [0.0f32; 2];
                for side in 0..2 {
                    let (o, ph) = (&oscs[side], &mut v.phase[side]);
                    let step = |n: usize| hz * o[n].detune;
                    // Oscillator 3 has nothing under it.
                    let mut s = wave(o[2].shape, ph[2], seed) * o[2].volume;
                    let wrapped_3 = (ph[2] + step(2)).floor() > ph[2].floor();
                    ph[2] += step(2);
                    ph[2] -= ph[2].floor();
                    // Oscillator 2 over 3, then 1 over that.
                    let mut sub_wrapped = wrapped_3;
                    for (n, algo) in [(1usize, mod_2), (0, mod_1)] {
                        let own = |phase: f32, seed: &mut u32| {
                            wave(o[n].shape, phase, seed) * o[n].volume
                        };
                        let before = ph[n];
                        s = match algo {
                            0 => own(ph[n] + s, seed),
                            1 => s * own(ph[n], seed),
                            2 => s + own(ph[n], seed),
                            3 => {
                                // The oscillator below began a period: so does this one.
                                if sub_wrapped {
                                    ph[n] = o[n].phase_offset;
                                }
                                own(ph[n], seed)
                            }
                            _ => {
                                ph[n] += s * fm_scale;
                                own(ph[n], seed)
                            }
                        };
                        ph[n] += step(n);
                        sub_wrapped = ph[n].floor() > before.floor();
                        ph[n] -= ph[n].floor();
                    }
                    out[side] = s;
                }
                *l += out[0] * level;
                *r += out[1] * level;
            }
            if v.env.is_idle() {
                self.alloc.idle(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_notes::{peak, pitch, play, sounds_and_falls_silent, tuned_to};
    use super::super::{open_native_instrument, BlockProcessor};
    use super::*;

    #[test]
    fn triple_osc_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 2.0);
        sounds_and_falls_silent(ID, 44100, &[], 2.0);
    }

    /// The defaults for a parameter list with only oscillator 1 sounding, at `coarse`.
    fn only_osc_1(coarse: f32) -> Vec<f32> {
        let mut values: Vec<f32> = params().iter().map(|p| p.default).collect();
        values[OSC_PARAMS] = 0.0;
        values[2 * OSC_PARAMS] = 0.0;
        values[2] = coarse;
        values[SUSTAIN] = 100.0;
        values
    }

    #[test]
    fn the_note_and_the_detunings_set_the_pitch() {
        for (note, coarse, want) in [
            (69u8, 0.0f32, 440.0f64),
            (57, 0.0, 220.0),
            (69, -12.0, 220.0),
        ] {
            let Some(mut inst) = open_native_instrument(ID, 48000, &only_osc_1(coarse)) else {
                panic!("does not open")
            };
            let (l, r) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 100)]);
            assert_eq!(l, r);
            let got = pitch(&l[24000..], 48000.0, 60.0, 1500.0);
            assert!(
                (got / want - 1.0).abs() < 0.005,
                "{note} at {coarse}: {got} Hz"
            );
        }
        // The fine detunings are per channel: 100 cents up on the right alone.
        let mut values = only_osc_1(0.0);
        values[4] = 100.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        let (l, r) = play(&mut inst, 48000, 256, &[(0, 0x90, 69, 100)]);
        let (a, b) = (
            pitch(&l[24000..], 48000.0, 60.0, 1500.0),
            pitch(&r[24000..], 48000.0, 60.0, 1500.0),
        );
        assert!(
            (a / 440.0 - 1.0).abs() < 0.005 && (b / 466.16 - 1.0).abs() < 0.005,
            "{a} {b}"
        );
    }

    /// The three default oscillators are an octave apart; pan moves one to a side.
    #[test]
    fn the_default_is_three_octaves_and_pan_takes_a_side() {
        let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
            panic!("does not open")
        };
        let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, 81, 100)]);
        let x = &l[12000..36000];
        for hz in [880.0, 440.0, 220.0] {
            assert!(tuned_to(x, hz, 48000.0) > 3.0, "{hz}");
        }
        let mut values = only_osc_1(0.0);
        values[1] = 100.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        let (l, r) = play(&mut inst, 12000, 256, &[(0, 0x90, 69, 100)]);
        assert!(peak(&l) == 0.0 && peak(&r) > 0.05);
    }

    /// Each way of modulating gives a sound of its own, all finite; Sync locks oscillator 1 to
    /// oscillator 2's period.
    #[test]
    fn the_five_modulations_differ_and_sync_follows_the_oscillator_below() {
        let mut sums = Vec::new();
        for algo in 0..5 {
            let mut values: Vec<f32> = params().iter().map(|p| p.default).collect();
            values[MOD_1] = algo as f32;
            values[OSC_PARAMS] = 100.0;
            values[2 * OSC_PARAMS] = 0.0;
            values[SUSTAIN] = 100.0;
            // Oscillator 1 a fifth above the note, oscillator 2 at it.
            values[2] = 7.0;
            values[OSC_PARAMS + 2] = 0.0;
            let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, 57, 100)]);
            assert!(l.iter().all(|v| v.is_finite()) && peak(&l) > 0.01, "{algo}");
            sums.push((l[24000..].iter().map(|v| v.abs()).sum::<f32>() * 10.0) as i64);
            if algo == 3 {
                // Reset every period of oscillator 2: the period heard is 220 Hz, not 330.
                let got = pitch(&l[24000..], 48000.0, 60.0, 1500.0);
                assert!((got / 220.0 - 1.0).abs() < 0.01, "synced: {got} Hz");
            }
        }
        sums.sort_unstable();
        sums.dedup();
        assert_eq!(sums.len(), 5, "{sums:?}");
        let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
            panic!("does not open")
        };
        inst.set_parameter(LEVEL, 0.0);
    }
}
