// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The synthesis models are the vendored crate
// crates/mi-plaits-dsp (Copyright (c) 2022 Oliver Rockstedt, MIT, a Rust port of MIT-licensed
// code (c) 2016-2021 by Emilie Gillet; https://github.com/sourcebox/mi-plaits-dsp-rs at commit
// d615458), unchanged; see its NOTICE.

//! Pro Wave Macro Oscillator: 24 synthesis models behind three controls (Harmonics, Timbre,
//! Morph), played polyphonically.
//!
//! Logic Pro equivalent: none directly; nearest are Retro Synth's four engines and Sculpture.
//!
//! Each voice is one `mi_plaits_dsp` voice at 48 kHz, the rate the models were written for,
//! rendered 12 frames at a time as on the module and converted to the project's rate
//! ([`FixedRate`]). A key holds the voice's trigger high. With Envelope on Gate the voice's
//! level follows the key through Attack and Release; on Decay the voice's own low-pass gate
//! is struck and rings out by Decay and Colour. The three Env knobs send the voice's own
//! decay envelope to pitch, Timbre and Morph. Out and Aux are the voice's two outputs.

use super::rate::FixedRate;
use super::voices::Voices;
use super::{stored, NativeInst};
use crate::native_fx::NativeParam;
use mi_plaits_dsp::voice::{Modulations, Patch, Voice};

pub const ID: &str = "prowave:macro-osc";
pub const NAME: &str = "Pro Wave Macro Oscillator";

pub const MODEL: usize = 0;
pub const HARMONICS: usize = 1;
pub const TIMBRE: usize = 2;
pub const MORPH: usize = 3;
pub const ENVELOPE: usize = 4;
pub const ATTACK: usize = 5;
pub const RELEASE: usize = 6;
pub const DECAY: usize = 7;
pub const COLOUR: usize = 8;
pub const PITCH_ENV: usize = 9;
pub const TIMBRE_ENV: usize = 10;
pub const MORPH_ENV: usize = 11;
pub const AUX: usize = 12;
pub const TUNE: usize = 13;
pub const VOICES: usize = 14;
pub const LEVEL: usize = 15;
pub const COUNT: usize = 16;

/// The models, in the engine's order.
pub static MODELS: [&str; 24] = [
    "Analog Filter",
    "Phase Distortion",
    "6-Op FM 1",
    "6-Op FM 2",
    "6-Op FM 3",
    "Wave Terrain",
    "String Machine",
    "Chiptune",
    "Virtual Analog",
    "Waveshaper",
    "2-Op FM",
    "Granular Formant",
    "Harmonic",
    "Wavetable",
    "Chords",
    "Speech",
    "Swarm",
    "Filtered Noise",
    "Particle Noise",
    "String",
    "Modal",
    "Bass Drum",
    "Snare Drum",
    "Hi-Hat",
];

pub const ENV_GATE: i32 = 0;
pub const ENV_DECAY: i32 = 1;

/// Most voices at once.
pub const MAX_VOICES: usize = 8;

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::choice("Model", 0, 8, &MODELS),
    NativeParam::knob("Harmonics", 0, 100, 50, "%"),
    NativeParam::knob("Timbre", 0, 100, 50, "%"),
    NativeParam::knob("Morph", 0, 100, 50, "%"),
    NativeParam::choice("Envelope", 0, ENV_GATE, &["Gate", "Decay"]),
    NativeParam::knob("Attack", 0, 2000, 2, "ms"),
    NativeParam::knob("Release", 0, 5000, 200, "ms"),
    NativeParam::knob("Decay", 0, 100, 50, "%"),
    NativeParam::knob("Colour", 0, 100, 50, "%"),
    NativeParam::knob("Pitch Env", -100, 100, 0, "%"),
    NativeParam::knob("Timbre Env", -100, 100, 0, "%"),
    NativeParam::knob("Morph Env", -100, 100, 0, "%"),
    NativeParam::knob("Aux", 0, 100, 0, "%"),
    NativeParam::knob("Tune", -24, 24, 0, "st"),
    NativeParam::knob("Voices", 1, MAX_VOICES as i32, 6, ""),
    NativeParam::knob("Level", -48, 12, -6, "dB"),
];

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        MODEL => MODELS
            .get(v.max(0) as usize)
            .map_or_else(String::new, |s| (*s).to_string()),
        ENVELOPE => if v == ENV_DECAY { "Decay" } else { "Gate" }.to_string(),
        ATTACK | RELEASE => format!("{v}ms"),
        TUNE => format!("{v}st"),
        VOICES => v.to_string(),
        LEVEL => format!("{v}dB"),
        _ if k < COUNT => format!("{v}%"),
        _ => String::new(),
    }
}

/// The rate the models run at.
const INNER_RATE: u32 = 48000;
/// Frames a voice renders at a time (the module's block).
const BLOCK: usize = 12;
/// A voice below this level (-60 dB) for `QUIET_BLOCKS` blocks (0.1 s) after its key went up
/// has rung out; so has one whose key's level has fallen to nothing. What is left of it (the
/// models' gate never closes entirely) is faded out, and the voice is free.
const QUIET: f32 = 1e-3;
const QUIET_BLOCKS: u32 = 400;
/// A voice still sounding this many blocks after its key went up is faded out too (30 s).
const LONGEST_TAIL_BLOCKS: u32 = 120_000;
/// Blocks the fade takes (10 ms).
const FADE_BLOCKS: f32 = 40.0;

struct OscVoice {
    voice: Voice<'static>,
    note: f32,
    velocity: f32,
    /// The key is down.
    gate: bool,
    /// One block with the trigger low before it goes high: a new note on a held voice.
    retrigger: bool,
    /// The key's level through Attack and Release, 0 to 1.
    env: f32,
    quiet: u32,
    /// Blocks since the key went up.
    since_up: u32,
    /// The fade at the end of a tail, 1 to 0.
    fade: f32,
}

/// The instrument.
pub struct MacroOsc {
    applied: [i32; COUNT],
    voices: Vec<OscVoice>,
    alloc: Voices<MAX_VOICES>,
    fixed: FixedRate,
    bend: f32,
}

impl MacroOsc {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let voices = (0..MAX_VOICES)
            .map(|_| {
                let mut voice = Voice::new(BLOCK, INNER_RATE as f32);
                voice.init();
                OscVoice {
                    voice,
                    note: 60.0,
                    velocity: 0.0,
                    gate: false,
                    retrigger: false,
                    env: 0.0,
                    quiet: 0,
                    since_up: 0,
                    fade: 1.0,
                }
            })
            .collect();
        MacroOsc {
            applied: stored(&PARAMS, values),
            voices,
            alloc: Voices::new(),
            fixed: FixedRate::new(INNER_RATE, rate, BLOCK),
            bend: 0.0,
        }
    }

    fn limit(&self) -> usize {
        (self.applied[VOICES].max(1) as usize).min(MAX_VOICES)
    }
}

/// One block of every sounding voice, summed into `left` and `right` (the same signal).
fn render_block(
    voices: &mut [OscVoice],
    alloc: &mut Voices<MAX_VOICES>,
    p: &[i32; COUNT],
    bend: f32,
    left: &mut [f32],
    right: &mut [f32],
) {
    let unit = |k: usize| p[k] as f32 / 100.0;
    let decay_mode = p[ENVELOPE] == ENV_DECAY;
    let block_seconds = BLOCK as f32 / INNER_RATE as f32;
    let step = |ms: i32| {
        if ms <= 0 {
            1.0
        } else {
            (block_seconds / (ms as f32 * 0.001)).min(1.0)
        }
    };
    let (up, down) = (step(p[ATTACK]), step(p[RELEASE]));
    let aux_mix = unit(AUX);
    let gain = 0.5 * 10f32.powf(p[LEVEL] as f32 / 20.0);
    left.fill(0.0);
    let mut patch = Patch {
        harmonics: unit(HARMONICS),
        timbre: unit(TIMBRE),
        morph: unit(MORPH),
        frequency_modulation_amount: unit(PITCH_ENV),
        timbre_modulation_amount: unit(TIMBRE_ENV),
        morph_modulation_amount: unit(MORPH_ENV),
        engine: p[MODEL].clamp(0, 23) as usize,
        decay: unit(DECAY),
        lpg_colour: unit(COLOUR),
        ..Patch::default()
    };
    let (mut out, mut aux) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
    for (i, v) in voices.iter_mut().enumerate() {
        if !alloc.is_busy(i) {
            continue;
        }
        if v.gate {
            v.env = (v.env + up).min(1.0);
        } else {
            v.env = (v.env - down).max(0.0);
        }
        patch.note = (v.note + p[TUNE] as f32).clamp(-119.0, 120.0);
        let trigger = v.gate && !std::mem::take(&mut v.retrigger);
        let modulations = Modulations {
            note: bend,
            trigger: if trigger { 1.0 } else { 0.0 },
            level: v.env * v.velocity,
            trigger_patched: true,
            level_patched: !decay_mode,
            ..Modulations::default()
        };
        v.voice.render(&patch, &modulations, &mut out, &mut aux);
        // On Decay the voice's own gate shapes the note: the key's velocity is its level.
        let level = gain * v.fade * if decay_mode { v.velocity } else { 1.0 };
        let mut peak = 0.0f32;
        for ((o, a), l) in out.iter().zip(&aux).zip(left.iter_mut()) {
            let y = o + (a - o) * aux_mix;
            peak = peak.max(y.abs());
            *l += y * level;
        }
        if v.gate {
            continue;
        }
        v.since_up = v.since_up.saturating_add(1);
        v.quiet = if peak < QUIET { v.quiet + 1 } else { 0 };
        let closed = !decay_mode && v.env <= 0.0;
        if closed || v.quiet >= QUIET_BLOCKS || v.since_up > LONGEST_TAIL_BLOCKS {
            v.fade = (v.fade - 1.0 / FADE_BLOCKS).max(0.0);
        }
        if v.fade <= 0.0 {
            alloc.idle(i);
        }
    }
    right.copy_from_slice(left);
}

impl NativeInst for MacroOsc {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = PARAMS.get(index) {
            self.applied[index] = p.stored(value);
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        let i = self.alloc.start(note, self.limit());
        if let Some(v) = self.voices.get_mut(i) {
            // A voice taken while its key was down needs its trigger to fall first.
            v.retrigger = v.gate || v.env > 0.0;
            v.note = f32::from(note);
            v.velocity = f32::from(velocity) / 127.0;
            v.gate = true;
            v.quiet = 0;
            v.since_up = 0;
            v.fade = 1.0;
        }
    }

    fn note_off(&mut self, note: u8) {
        if let Some(v) = self.alloc.stop(note).and_then(|i| self.voices.get_mut(i)) {
            v.gate = false;
        }
    }

    fn all_notes_off(&mut self) {
        let voices = &mut self.voices;
        self.alloc.stop_all(|i| {
            if let Some(v) = voices.get_mut(i) {
                v.gate = false;
            }
        });
    }

    fn all_sound_off(&mut self) {
        self.alloc.clear();
        for v in &mut self.voices {
            v.gate = false;
            v.env = 0.0;
        }
        self.fixed.reset();
    }

    fn bend(&mut self, semitones: f32) {
        self.bend = semitones;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let MacroOsc {
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
    use super::super::test_notes::{peak, pitch, play, sounds_and_falls_silent};
    use super::super::{open_native_instrument, BlockProcessor};
    use super::*;

    #[test]
    fn macro_osc_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 1.0);
        sounds_and_falls_silent(ID, 44100, &[], 1.0);
    }

    /// Every model sounds on a held note at the defaults.
    #[test]
    fn every_model_sounds() {
        for (m, name) in MODELS.iter().enumerate() {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[m as f32]) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 24000, 256, &[(0, 0x90, 48, 110)]);
            assert!(l.iter().all(|v| v.is_finite()), "{name}");
            assert!(peak(&l) > 0.005, "{name}: {}", peak(&l));
        }
    }

    #[test]
    fn the_note_sets_the_pitch() {
        for rate in [48000u32, 44100] {
            for (note, hz) in [(57u8, 220.0f64), (69, 440.0)] {
                let Some(mut inst) = open_native_instrument(ID, rate, &[]) else {
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

    /// On Decay a struck note dies away by itself while the key stays down.
    #[test]
    fn decay_rings_out_under_a_held_key() {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[ENVELOPE] = ENV_DECAY as f32;
        values[DECAY] = 20.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        let (l, _) = play(&mut inst, 96000, 256, &[(0, 0x90, 60, 100)]);
        assert!(peak(&l[..12000]) > 0.02, "{}", peak(&l[..12000]));
        assert!(peak(&l[84000..]) < peak(&l[..12000]) * 0.05);
    }

    /// The Voices setting limits how many notes sound, and a parameter moved while playing
    /// is heard (the level).
    #[test]
    fn voices_and_level_act_while_it_plays() {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[VOICES] = 1.0;
        let Some(mut one) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        let chord = [(0, 0x90, 48, 100), (0, 0x90, 55, 100), (0, 0x90, 64, 100)];
        let (a, _) = play(&mut one, 24000, 256, &chord);
        let Some(mut six) = open_native_instrument(ID, 48000, &[]) else {
            panic!("does not open")
        };
        let (b, _) = play(&mut six, 24000, 256, &chord);
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        assert!(rms(&b[12000..]) > rms(&a[12000..]) * 1.3);
        six.set_parameter(LEVEL, -30.0);
        let (c, _) = play(&mut six, 24000, 256, &[]);
        assert!(rms(&c[12000..]) < rms(&b[12000..]) * 0.2);
    }
}
