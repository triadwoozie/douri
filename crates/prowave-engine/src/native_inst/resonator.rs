// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The resonator is the vendored crate crates/mi-dsp
// (MIT, Copyright 2015 Emilie Gillet, https://github.com/pichenettes/eurorack, rings/dsp at
// commit f4cfa76), its C++ unchanged; see its NOTICE.

//! Pro Wave Resonator: struck and plucked strings, plates and bars from a modal or string
//! resonator, played from the keyboard.
//!
//! Logic Pro equivalent: Sculpture.
//!
//! One resonator object (`mi_dsp::Resonator`) at 48 kHz, 24 frames at a time, converted to
//! the project's rate ([`FixedRate`]). Each key strikes it with its own exciter; with 2 or 4
//! voices the resonator itself gives each new note the next voice and lets the others ring.
//! Notes ring out by Damping: letting a key go does nothing. The resonator's two outputs (odd
//! and even partials, or alternate voices) are the left and right channels. While nothing
//! sounds the resonator is not run.

use super::rate::FixedRate;
use super::{stored, NativeInst};
use crate::native_fx::NativeParam;
use mi_dsp::{Resonator as Dsp, ResonatorBlock, RESONATOR_BLOCK, RESONATOR_RATE};

pub const ID: &str = "prowave:resonator";
pub const NAME: &str = "Pro Wave Resonator";

pub const MODEL: usize = 0;
pub const VOICES: usize = 1;
pub const STRUCTURE: usize = 2;
pub const BRIGHTNESS: usize = 3;
pub const DAMPING: usize = 4;
pub const POSITION: usize = 5;
pub const CHORD: usize = 6;
pub const TUNE: usize = 7;
pub const LEVEL: usize = 8;
pub const COUNT: usize = 9;

pub static MODELS: [&str; 6] = [
    "Modal",
    "Sympathetic Strings",
    "String",
    "FM Voice",
    "Quantized Strings",
    "String and Reverb",
];

/// The Voices choice's values.
pub static VOICE_COUNTS: [&str; 3] = ["1", "2", "4"];

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::choice("Model", 0, 0, &MODELS),
    NativeParam::choice("Voices", 0, 2, &VOICE_COUNTS),
    NativeParam::knob("Structure", 0, 100, 25, "%"),
    NativeParam::knob("Brightness", 0, 100, 50, "%"),
    NativeParam::knob("Damping", 0, 100, 40, "%"),
    NativeParam::knob("Position", 0, 100, 30, "%"),
    NativeParam::knob("Chord", 1, 11, 1, ""),
    NativeParam::knob("Tune", -24, 24, 0, "st"),
    NativeParam::knob("Level", -48, 12, 0, "dB"),
];

pub fn value_text(k: usize, v: i32) -> String {
    let pick = |names: &[&str]| {
        names
            .get(v.max(0) as usize)
            .map_or_else(String::new, |s| (*s).to_string())
    };
    match k {
        MODEL => pick(&MODELS),
        VOICES => pick(&VOICE_COUNTS),
        CHORD => v.to_string(),
        TUNE => format!("{v}st"),
        LEVEL => format!("{v}dB"),
        _ if k < COUNT => format!("{v}%"),
        _ => String::new(),
    }
}

/// The resonator's polyphony for the Voices choice.
pub(crate) fn polyphony(choice: i32) -> i32 {
    match choice {
        0 => 1,
        1 => 2,
        _ => 4,
    }
}

/// Notes waiting to be struck: the resonator takes one a block (half a millisecond).
const MAX_PENDING: usize = 32;
/// Blocks below `QUIET` after which the resonator is not run until the next note (1 s).
const QUIET_BLOCKS: u32 = 2000;
const QUIET: f32 = 1e-5;

struct State {
    dsp: Dsp,
    pending: [(u8, u8); MAX_PENDING],
    pending_len: usize,
    note: f32,
    velocity: f32,
    quiet: u32,
    asleep: bool,
}

/// The instrument.
pub struct Resonator {
    applied: [i32; COUNT],
    state: Option<State>,
    fixed: FixedRate,
    bend: f32,
}

impl Resonator {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        Resonator {
            applied: stored(&PARAMS, values),
            // Out of memory: the instrument is silent.
            state: Dsp::new().map(|dsp| State {
                dsp,
                pending: [(0, 0); MAX_PENDING],
                pending_len: 0,
                note: 60.0,
                velocity: 1.0,
                quiet: 0,
                asleep: true,
            }),
            fixed: FixedRate::new(RESONATOR_RATE, rate, RESONATOR_BLOCK),
            bend: 0.0,
        }
    }
}

fn render_block(s: &mut State, p: &[i32; COUNT], bend: f32, left: &mut [f32], right: &mut [f32]) {
    let strum = s.pending_len > 0;
    if strum {
        let (note, velocity) = s.pending[0];
        s.pending.copy_within(1..s.pending_len, 0);
        s.pending_len -= 1;
        s.note = f32::from(note);
        s.velocity = f32::from(velocity) / 127.0;
        s.asleep = false;
        s.quiet = 0;
    }
    if s.asleep {
        left.fill(0.0);
        right.fill(0.0);
        return;
    }
    let unit = |k: usize| p[k] as f32 / 100.0;
    let block = ResonatorBlock {
        model: p[MODEL],
        polyphony: polyphony(p[VOICES]),
        structure: unit(STRUCTURE),
        brightness: unit(BRIGHTNESS),
        damping: unit(DAMPING),
        position: unit(POSITION),
        note: s.note,
        fm: p[TUNE] as f32 + bend,
        chord: p[CHORD] - 1,
        strum: i32::from(strum),
        external: 0,
    };
    s.dsp.process(&block, None, left, right);
    // The exciter has no level of its own: the key's velocity scales what comes out.
    let gain = 10f32.powf(p[LEVEL] as f32 / 20.0) * (0.3 + 0.7 * s.velocity);
    let mut peak = 0.0f32;
    for v in left.iter_mut().chain(right.iter_mut()) {
        peak = peak.max(v.abs());
        *v *= gain;
    }
    s.quiet = if peak < QUIET { s.quiet + 1 } else { 0 };
    if s.quiet >= QUIET_BLOCKS {
        s.asleep = true;
        s.dsp.reset();
    }
}

impl NativeInst for Resonator {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = PARAMS.get(index) {
            self.applied[index] = p.stored(value);
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if let Some(s) = self.state.as_mut() {
            if s.pending_len < MAX_PENDING {
                s.pending[s.pending_len] = (note, velocity);
                s.pending_len += 1;
            }
        }
    }

    fn note_off(&mut self, _note: u8) {}

    fn all_notes_off(&mut self) {
        if let Some(s) = self.state.as_mut() {
            s.pending_len = 0;
        }
    }

    fn all_sound_off(&mut self) {
        if let Some(s) = self.state.as_mut() {
            s.pending_len = 0;
            s.asleep = true;
            s.dsp.reset();
        }
        self.fixed.reset();
    }

    fn bend(&mut self, semitones: f32) {
        self.bend = semitones;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let Resonator {
            applied,
            state,
            fixed,
            bend,
        } = self;
        match state.as_mut() {
            Some(s) => fixed.pull(left, right, |l, r| render_block(s, applied, *bend, l, r)),
            None => {
                left.fill(0.0);
                right.fill(0.0);
            }
        }
    }

    fn latency_seconds(&self) -> f64 {
        self.fixed.lag_seconds()
    }
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, play, sounds_and_falls_silent, tuned_to};
    use super::*;

    #[test]
    fn resonator_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 12.0);
        sounds_and_falls_silent(ID, 44100, &[], 12.0);
    }

    #[test]
    fn every_model_sounds_at_every_voice_count() {
        for (m, name) in MODELS.iter().enumerate() {
            for voices in 0..3 {
                let values = [m as f32, voices as f32];
                let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                    panic!("does not open")
                };
                let chord = [(0, 0x90, 48, 110), (0, 0x90, 55, 110), (0, 0x90, 64, 110)];
                let (l, r) = play(&mut inst, 24000, 256, &chord);
                assert!(l.iter().chain(&r).all(|v| v.is_finite()), "{name}");
                assert!(peak(&l).max(peak(&r)) > 0.005, "{name} with {voices}");
            }
        }
    }

    /// The plain string is tuned to the note: its partials are the note's harmonics.
    #[test]
    fn the_note_sets_the_pitch() {
        for (note, hz) in [(45u8, 110.0f64), (57, 220.0)] {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[2.0, 0.0]) else {
                panic!("does not open")
            };
            let (l, r) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 100)]);
            let sum: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a + b).collect();
            let ratio = tuned_to(&sum[4800..28800], hz, 48000.0);
            assert!(ratio > 5.0, "note {note}: {ratio}");
        }
    }
}
