// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The oscillator is the vendored crate crates/mi-dsp
// (MIT, Copyright 2012 Emilie Gillet, https://github.com/pichenettes/eurorack, braids at commit
// f4cfa76), its C++ unchanged; see its NOTICE.

//! Pro Wave Digital Oscillator: 47 oscillator shapes with two controls each (Timbre and
//! Colour), played polyphonically through an envelope.
//!
//! Logic Pro equivalent: none directly; nearest are ES2's Digiwaves and Retro Synth's Table
//! engine.
//!
//! Each voice is one oscillator object (`mi_dsp::Oscillator`) at 96 kHz, 24 frames at a time
//! as on the module, with Pro Wave's envelope ([`Adsr`]) as its level; the sum is converted to
//! the project's rate ([`FixedRate`]). A key down strikes the oscillator (the plucked, struck
//! and drum shapes start there). The output is mono, on both channels.

use super::envelope::{Adsr, AdsrRates, AdsrTimes};
use super::rate::FixedRate;
use super::voices::Voices;
use super::{stored, NativeInst};
use crate::native_fx::NativeParam;
use mi_dsp::{Oscillator, OSCILLATOR_BLOCK, OSCILLATOR_RATE, OSCILLATOR_SHAPES};

pub const ID: &str = "prowave:digital-osc";
pub const NAME: &str = "Dōri Digital Oscillator";

pub const SHAPE: usize = 0;
pub const TIMBRE: usize = 1;
pub const COLOUR: usize = 2;
pub const ATTACK: usize = 3;
pub const DECAY: usize = 4;
pub const SUSTAIN: usize = 5;
pub const RELEASE: usize = 6;
pub const TUNE: usize = 7;
pub const VOICES: usize = 8;
pub const LEVEL: usize = 9;
pub const COUNT: usize = 10;

/// The shapes, in the oscillator's order.
pub static SHAPES: [&str; OSCILLATOR_SHAPES] = [
    "Saw Notch",
    "Morph",
    "Saw Square",
    "Sine Triangle",
    "Buzz",
    "Square Sub",
    "Saw Sub",
    "Square Sync",
    "Saw Sync",
    "Triple Saw",
    "Triple Square",
    "Triple Triangle",
    "Triple Sine",
    "Triple Ring Mod",
    "Saw Swarm",
    "Saw Comb",
    "Toy",
    "Filter Low Pass",
    "Filter Peak",
    "Filter Band Pass",
    "Filter High Pass",
    "Vosim",
    "Vowel",
    "Vowel FOF",
    "Harmonics",
    "FM",
    "Feedback FM",
    "Chaotic FM",
    "Plucked",
    "Bowed",
    "Blown",
    "Fluted",
    "Bell",
    "Drum",
    "Kick",
    "Cymbal",
    "Snare",
    "Wavetables",
    "Wave Map",
    "Wave Line",
    "Wave Paraphonic",
    "Filtered Noise",
    "Twin Peaks Noise",
    "Clocked Noise",
    "Granular Cloud",
    "Particle Noise",
    "Digital Modulation",
];

/// Most voices at once.
pub const MAX_VOICES: usize = 6;

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::choice("Shape", 0, 0, &SHAPES),
    NativeParam::knob("Timbre", 0, 100, 50, "%"),
    NativeParam::knob("Colour", 0, 100, 50, "%"),
    NativeParam::knob("Attack", 0, 2000, 2, "ms"),
    NativeParam::knob("Decay", 0, 5000, 400, "ms"),
    NativeParam::knob("Sustain", 0, 100, 70, "%"),
    NativeParam::knob("Release", 0, 5000, 250, "ms"),
    NativeParam::knob("Tune", -24, 24, 0, "st"),
    NativeParam::knob("Voices", 1, MAX_VOICES as i32, MAX_VOICES as i32, ""),
    NativeParam::knob("Level", -48, 12, -6, "dB"),
];

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        SHAPE => SHAPES
            .get(v.max(0) as usize)
            .map_or_else(String::new, |s| (*s).to_string()),
        ATTACK | DECAY | RELEASE => format!("{v}ms"),
        TUNE => format!("{v}st"),
        VOICES => v.to_string(),
        LEVEL => format!("{v}dB"),
        _ if k < COUNT => format!("{v}%"),
        _ => String::new(),
    }
}

struct OscVoice {
    osc: Oscillator,
    env: Adsr,
    note: f32,
    velocity: f32,
    strike: bool,
}

/// The instrument.
pub struct DigitalOsc {
    applied: [i32; COUNT],
    voices: Vec<OscVoice>,
    alloc: Voices<MAX_VOICES>,
    fixed: FixedRate,
    bend: f32,
}

impl DigitalOsc {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        // Out of memory: fewer voices, or none.
        let voices = (0..MAX_VOICES)
            .filter_map(|_| {
                Some(OscVoice {
                    osc: Oscillator::new()?,
                    env: Adsr::new(),
                    note: 60.0,
                    velocity: 0.0,
                    strike: false,
                })
            })
            .collect();
        DigitalOsc {
            applied: stored(&PARAMS, values),
            voices,
            alloc: Voices::new(),
            fixed: FixedRate::new(OSCILLATOR_RATE, rate, OSCILLATOR_BLOCK),
            bend: 0.0,
        }
    }

    fn limit(&self) -> usize {
        (self.applied[VOICES].max(1) as usize).min(self.voices.len().max(1))
    }
}

fn render_block(
    voices: &mut [OscVoice],
    alloc: &mut Voices<MAX_VOICES>,
    p: &[i32; COUNT],
    bend: f32,
    left: &mut [f32],
    right: &mut [f32],
) {
    let rates = AdsrRates::new(
        &AdsrTimes {
            attack_s: p[ATTACK] as f32 * 0.001,
            decay_s: p[DECAY] as f32 * 0.001,
            sustain: p[SUSTAIN] as f32 / 100.0,
            release_s: p[RELEASE] as f32 * 0.001,
        },
        OSCILLATOR_RATE as f32,
    );
    let to_15 = |k: usize| (p[k] * 32767 / 100).clamp(0, 32767);
    let gain = 0.5 * 10f32.powf(p[LEVEL] as f32 / 20.0) / 32768.0;
    left.fill(0.0);
    let mut block = [0i16; OSCILLATOR_BLOCK];
    for (i, v) in voices.iter_mut().enumerate() {
        if !alloc.is_busy(i) {
            continue;
        }
        let pitch = ((v.note + p[TUNE] as f32 + bend) * 128.0).round() as i32;
        let strike = std::mem::take(&mut v.strike);
        v.osc.render(
            p[SHAPE],
            pitch,
            to_15(TIMBRE),
            to_15(COLOUR),
            strike,
            &mut block,
        );
        let level = gain * v.velocity;
        for (x, l) in block.iter().zip(left.iter_mut()) {
            *l += f32::from(*x) * level * rates.tick(&mut v.env);
        }
        if v.env.is_idle() {
            alloc.idle(i);
        }
    }
    right.copy_from_slice(left);
}

impl NativeInst for DigitalOsc {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = PARAMS.get(index) {
            self.applied[index] = p.stored(value);
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if self.voices.is_empty() {
            return;
        }
        let i = self.alloc.start(note, self.limit());
        if let Some(v) = self.voices.get_mut(i) {
            v.note = f32::from(note);
            v.velocity = f32::from(velocity) / 127.0;
            v.strike = true;
            v.env.gate_on();
        }
    }

    fn note_off(&mut self, note: u8) {
        if let Some(v) = self.alloc.stop(note).and_then(|i| self.voices.get_mut(i)) {
            v.env.gate_off();
        }
    }

    fn all_notes_off(&mut self) {
        let voices = &mut self.voices;
        self.alloc.stop_all(|i| {
            if let Some(v) = voices.get_mut(i) {
                v.env.gate_off();
            }
        });
    }

    fn all_sound_off(&mut self) {
        self.alloc.clear();
        for v in &mut self.voices {
            v.env.kill();
        }
        self.fixed.reset();
    }

    fn bend(&mut self, semitones: f32) {
        self.bend = semitones;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let DigitalOsc {
            applied,
            voices,
            alloc,
            fixed,
            bend,
        } = self;
        fixed.pull(left, right, |l, r| {
            render_block(voices, alloc, applied, *bend, l, r)
        });
    }

    fn latency_seconds(&self) -> f64 {
        self.fixed.lag_seconds()
    }
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, pitch, play, sounds_and_falls_silent};
    use super::*;

    #[test]
    fn digital_osc_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 2.0);
        sounds_and_falls_silent(ID, 44100, &[], 2.0);
    }

    #[test]
    fn every_shape_sounds() {
        for (m, name) in SHAPES.iter().enumerate() {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[m as f32]) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 12000, 256, &[(0, 0x90, 48, 110)]);
            assert!(l.iter().all(|v| v.is_finite()), "{name}");
            assert!(peak(&l) > 0.005, "{name}: {}", peak(&l));
        }
    }

    #[test]
    fn the_note_sets_the_pitch() {
        for rate in [48000u32, 44100] {
            for (note, hz) in [(57u8, 220.0f64), (69, 440.0)] {
                // Sine Triangle with no fold and no triangle: a sine at the note.
                let Some(mut inst) = open_native_instrument(ID, rate, &[3.0, 0.0, 0.0]) else {
                    panic!("does not open")
                };
                let n = rate as usize;
                let (l, _) = play(&mut inst, n, 256, &[(0, 0x90, note, 100)]);
                let got = pitch(&l[n / 2..], f64::from(rate), 60.0, 1500.0);
                assert!(
                    (got / hz - 1.0).abs() < 0.01,
                    "{rate}: note {note} is {got} Hz"
                );
            }
        }
    }
}
