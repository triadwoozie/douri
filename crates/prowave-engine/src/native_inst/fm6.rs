// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The FM core is the vendored crate crates/chip-dsp
// (vendor/msfa: Apache-2.0, Copyright 2012-2013 Google Inc., from
// github.com/google/music-synthesizer-for-android as collected and modified by
// github.com/kyorohiro/tetorica-soundchip-native at commit b4f2543), unchanged; see its NOTICE.

//! FM6: a six-operator FM synthesizer with 32 algorithms, whose voice is the 145 parameters
//! of a DX7 voice.
//!
//! Logic Pro equivalent: none with six operators (EFM1 has two); Retro Synth's FM engine.
//!
//! Sixteen notes of `chip_dsp::Fm6` at 44.1 kHz, the one rate the core's shared tables are
//! made for, 64 frames at a time, converted to the project's rate ([`FixedRate`]). Parameters
//! 0 to 144 are the voice's bytes in the order of the DX7's own voice dump (operators 6 down to
//! 1, 21 bytes each, then the pitch envelope, algorithm, feedback, LFO and transpose), with the
//! same ranges, so a voice from a .syx file is a list of parameter values ([`unpack_voice`]).
//! A change of the voice is heard on the next note, as on the instrument. No voice of the
//! original instrument is built in: the default is Pro Wave's own two-operator voice.

use super::rate::FixedRate;
use super::voices::Voices;
use super::NativeInst;
use crate::native_fx::NativeParam;
use chip_dsp::{Fm6 as Dsp, FM6_BLOCK, FM6_PACKED, FM6_RATE, FM6_SLOTS, FM6_VOICE};
use std::sync::OnceLock;

pub const ID: &str = "prowave:fm6";
pub const NAME: &str = "FM6";

/// Bytes of a voice that are parameters (the ten after them are its name).
pub const VOICE_PARAMS: usize = 145;
/// Parameters of one operator; operator `n` (1 to 6) starts at `(6 - n) * OP_PARAMS`.
pub const OP_PARAMS: usize = 21;
pub const OP_OUTPUT_LEVEL: usize = 16;
pub const OP_COARSE: usize = 18;
pub const PITCH_ENV: usize = 126;
pub const ALGORITHM: usize = 134;
pub const FEEDBACK: usize = 135;
pub const TRANSPOSE: usize = 144;
pub const VOICES: usize = 145;
pub const LEVEL: usize = 146;
pub const COUNT: usize = 147;

static CURVES: [&str; 4] = ["-Lin", "-Exp", "+Exp", "+Lin"];
static MODES: [&str; 2] = ["Ratio", "Fixed"];
static OFF_ON: [&str; 2] = ["Off", "On"];
static LFO_WAVES: [&str; 6] = [
    "Triangle",
    "Saw Down",
    "Saw Up",
    "Square",
    "Sine",
    "Sample and Hold",
];

/// One operator's parameters: (name, maximum, choices).
static OP_TABLE: [(&str, i32, &[&str]); OP_PARAMS] = [
    ("Rate 1", 99, &[]),
    ("Rate 2", 99, &[]),
    ("Rate 3", 99, &[]),
    ("Rate 4", 99, &[]),
    ("Level 1", 99, &[]),
    ("Level 2", 99, &[]),
    ("Level 3", 99, &[]),
    ("Level 4", 99, &[]),
    ("Break Point", 99, &[]),
    ("Left Depth", 99, &[]),
    ("Right Depth", 99, &[]),
    ("Left Curve", 3, &CURVES),
    ("Right Curve", 3, &CURVES),
    ("Rate Scaling", 7, &[]),
    ("Amp Mod Sens", 3, &[]),
    ("Velocity Sens", 7, &[]),
    ("Output Level", 99, &[]),
    ("Mode", 1, &MODES),
    ("Coarse", 31, &[]),
    ("Fine", 99, &[]),
    ("Detune", 14, &[]),
];

/// The parameters after the operators: (name, maximum, choices).
static GLOBAL_TABLE: [(&str, i32, &[&str]); VOICE_PARAMS - 6 * OP_PARAMS] = [
    ("Pitch Rate 1", 99, &[]),
    ("Pitch Rate 2", 99, &[]),
    ("Pitch Rate 3", 99, &[]),
    ("Pitch Rate 4", 99, &[]),
    ("Pitch Level 1", 99, &[]),
    ("Pitch Level 2", 99, &[]),
    ("Pitch Level 3", 99, &[]),
    ("Pitch Level 4", 99, &[]),
    ("Algorithm", 31, &[]),
    ("Feedback", 7, &[]),
    ("Osc Sync", 1, &OFF_ON),
    ("LFO Speed", 99, &[]),
    ("LFO Delay", 99, &[]),
    ("LFO Pitch Depth", 99, &[]),
    ("LFO Amp Depth", 99, &[]),
    ("LFO Sync", 1, &OFF_ON),
    ("LFO Wave", 5, &LFO_WAVES),
    ("LFO Pitch Sens", 7, &[]),
    ("Transpose", 48, &[]),
];

/// Pro Wave's default voice: operator 2 modulates operator 1 (algorithm 1), both falling
/// away, the modulator faster and louder with velocity.
fn default_voice() -> [i32; VOICE_PARAMS] {
    let mut v = [0i32; VOICE_PARAMS];
    for op in 0..6 {
        let o = op * OP_PARAMS;
        v[o..o + 4].copy_from_slice(&[99, 99, 99, 99]);
        v[o + 4..o + 8].copy_from_slice(&[99, 99, 99, 0]);
        v[o + 8] = 39;
        v[o + OP_COARSE] = 1;
        v[o + 20] = 7;
    }
    // Operator 1, the carrier.
    let c = 5 * OP_PARAMS;
    v[c..c + 4].copy_from_slice(&[96, 35, 30, 55]);
    v[c + 4..c + 8].copy_from_slice(&[99, 85, 0, 0]);
    v[c + 15] = 2;
    v[c + OP_OUTPUT_LEVEL] = 99;
    // Operator 2, its modulator.
    let m = 4 * OP_PARAMS;
    v[m..m + 4].copy_from_slice(&[95, 50, 35, 60]);
    v[m + 4..m + 8].copy_from_slice(&[99, 60, 0, 0]);
    v[m + 15] = 5;
    v[m + OP_OUTPUT_LEVEL] = 74;
    v[PITCH_ENV..PITCH_ENV + 4].copy_from_slice(&[99, 99, 99, 99]);
    v[PITCH_ENV + 4..PITCH_ENV + 8].copy_from_slice(&[50, 50, 50, 50]);
    v[FEEDBACK] = 3;
    v[136] = 1;
    v[137] = 35;
    v[141] = 1;
    v[143] = 3;
    v[TRANSPOSE] = 24;
    v
}

/// Every parameter, in index order.
pub fn params() -> &'static [NativeParam] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    static PARAMS: OnceLock<Vec<NativeParam>> = OnceLock::new();
    PARAMS.get_or_init(|| {
        let names = NAMES.get_or_init(|| {
            (0..6 * OP_PARAMS)
                .map(|k| format!("Op {} {}", 6 - k / OP_PARAMS, OP_TABLE[k % OP_PARAMS].0))
                .collect()
        });
        let defaults = default_voice();
        let mut out: Vec<NativeParam> = (0..VOICE_PARAMS)
            .map(|k| {
                let (name, max, choices): (&'static str, i32, &'static [&'static str]) =
                    if k < 6 * OP_PARAMS {
                        let (_, max, choices) = OP_TABLE[k % OP_PARAMS];
                        (names[k].as_str(), max, choices)
                    } else {
                        GLOBAL_TABLE[k - 6 * OP_PARAMS]
                    };
                if choices.is_empty() {
                    NativeParam::knob(name, 0, max, defaults[k], "")
                } else {
                    NativeParam::choice(name, 0, defaults[k], choices)
                }
            })
            .collect();
        out.push(NativeParam::knob("Voices", 1, FM6_SLOTS as i32, 8, ""));
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
        ALGORITHM => (v + 1).to_string(),
        TRANSPOSE => format!("{}st", v - 24),
        LEVEL => format!("{v}dB"),
        _ if k < 6 * OP_PARAMS && k % OP_PARAMS == 20 => (v - 7).to_string(),
        _ => v.to_string(),
    }
}

/// A voice as it is packed in a 32-voice bank, as parameter values 0 to 144 (each kept in
/// its parameter's range) and its name.
pub fn unpack_voice(packed: &[u8; FM6_PACKED]) -> (Vec<f32>, String) {
    let bytes = chip_dsp::fm6_unpack(packed);
    let values = params()
        .iter()
        .zip(&bytes[..VOICE_PARAMS])
        .map(|(p, b)| p.stored(f32::from(*b)) as f32)
        .collect();
    let name: String = bytes[VOICE_PARAMS..]
        .iter()
        .map(|b| {
            if (0x20..0x7f).contains(b) {
                *b as char
            } else {
                ' '
            }
        })
        .collect();
    (values, name.trim().to_string())
}

/// The voices of a .syx file: a 32-voice bank (4104 bytes, or its 4096 bytes of data alone),
/// as (name, parameter values 0 to 144). Empty for anything else.
pub fn bank_voices(file: &[u8]) -> Vec<(String, Vec<f32>)> {
    let data = match file {
        [0xf0, 0x43, _, 0x09, 0x20, 0x00, rest @ ..] if rest.len() >= 32 * FM6_PACKED => {
            &rest[..32 * FM6_PACKED]
        }
        d if d.len() == 32 * FM6_PACKED => d,
        _ => return Vec::new(),
    };
    data.as_chunks::<FM6_PACKED>()
        .0
        .iter()
        .filter_map(|packed| {
            // A data byte of a system-exclusive message has its top bit clear.
            if packed.iter().any(|b| *b > 0x7f) {
                return None;
            }
            let (values, name) = unpack_voice(packed);
            Some((name, values))
        })
        .collect()
}

/// Blocks below `QUIET` after its key went up at which a note is over (70 ms).
const QUIET_BLOCKS: u32 = 48;
const QUIET: f32 = 1e-4;

/// The instrument.
pub struct Fm6 {
    applied: [i32; COUNT],
    /// `None` when memory ran out: the instrument is silent.
    dsp: Option<Dsp>,
    alloc: Voices<FM6_SLOTS>,
    quiet: [u32; FM6_SLOTS],
    /// The voice has changed since the core was last given it.
    dirty: bool,
    fixed: FixedRate,
    bend: f32,
}

impl Fm6 {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let mut applied = [0i32; COUNT];
        for (k, p) in params().iter().enumerate() {
            applied[k] = p.stored(values.get(k).copied().unwrap_or(p.default));
        }
        Fm6 {
            applied,
            dsp: Dsp::new(),
            alloc: Voices::new(),
            quiet: [0; FM6_SLOTS],
            dirty: true,
            fixed: FixedRate::new(FM6_RATE, rate, FM6_BLOCK),
            bend: 0.0,
        }
    }

    fn limit(&self) -> usize {
        (self.applied[VOICES].max(1) as usize).min(FM6_SLOTS)
    }

    /// Gives the core the voice, if it changed.
    fn send_voice(&mut self) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        let Some(dsp) = self.dsp.as_mut() else { return };
        // The name's ten bytes are spaces.
        let mut voice = [b' '; FM6_VOICE];
        for (b, v) in voice.iter_mut().zip(&self.applied[..VOICE_PARAMS]) {
            *b = (*v).clamp(0, 127) as u8;
        }
        dsp.set_voice(&voice);
    }
}

impl NativeInst for Fm6 {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = params().get(index) {
            let v = p.stored(value);
            if self.applied[index] != v {
                self.applied[index] = v;
                self.dirty |= index < VOICE_PARAMS;
            }
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.send_voice();
        let limit = self.limit();
        let Some(dsp) = self.dsp.as_mut() else { return };
        let slot = self.alloc.start(note, limit);
        let played = (i32::from(note) + self.applied[TRANSPOSE] - 24).clamp(0, 127);
        dsp.note_on(slot, played as u8, velocity);
        self.quiet[slot] = 0;
    }

    fn note_off(&mut self, note: u8) {
        if let (Some(slot), Some(dsp)) = (self.alloc.stop(note), self.dsp.as_mut()) {
            dsp.note_off(slot);
        }
    }

    fn all_notes_off(&mut self) {
        let Some(dsp) = self.dsp.as_mut() else { return };
        self.alloc.stop_all(|slot| dsp.note_off(slot));
    }

    fn all_sound_off(&mut self) {
        self.alloc.clear();
        self.fixed.reset();
    }

    fn bend(&mut self, semitones: f32) {
        self.bend = semitones;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let Fm6 {
            applied,
            dsp,
            alloc,
            quiet,
            fixed,
            bend,
            ..
        } = self;
        let Some(dsp) = dsp.as_mut() else {
            left.fill(0.0);
            right.fill(0.0);
            return;
        };
        let gain = 0.5 * 10f32.powf(applied[LEVEL] as f32 / 20.0);
        fixed.pull(left, right, |l, r| {
            let mut mask = 0u32;
            for slot in 0..FM6_SLOTS {
                if alloc.is_busy(slot) {
                    mask |= 1 << slot;
                }
            }
            let (mut out, mut peaks) = ([0.0f32; FM6_BLOCK], [0.0f32; FM6_SLOTS]);
            if mask != 0 {
                dsp.render(mask, *bend, &mut out, &mut peaks);
            }
            for ((a, b), y) in l.iter_mut().zip(r.iter_mut()).zip(&out) {
                *a = y * gain;
                *b = *a;
            }
            for slot in 0..FM6_SLOTS {
                if mask & (1 << slot) == 0 || alloc.is_held(slot) {
                    continue;
                }
                quiet[slot] = if peaks[slot] < QUIET {
                    quiet[slot] + 1
                } else {
                    0
                };
                if quiet[slot] >= QUIET_BLOCKS {
                    alloc.idle(slot);
                }
            }
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
    fn fm6_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 3.0);
        sounds_and_falls_silent(ID, 44100, &[], 3.0);
    }

    #[test]
    fn the_table_is_the_voice_dump_and_texts_read_as_on_the_instrument() {
        let p = params();
        assert_eq!(p.len(), COUNT);
        assert_eq!(p[0].name, "Op 6 Rate 1");
        assert_eq!(p[5 * OP_PARAMS + OP_OUTPUT_LEVEL].name, "Op 1 Output Level");
        assert_eq!(p[ALGORITHM].name, "Algorithm");
        assert_eq!(p[FEEDBACK].name, "Feedback");
        assert_eq!(p[TRANSPOSE].name, "Transpose");
        assert_eq!(value_text(ALGORITHM, 0), "1");
        assert_eq!(value_text(TRANSPOSE, 24), "0st");
        assert_eq!(value_text(20, 7), "0");
        assert_eq!(value_text(11, 2), "+Exp");
        assert_eq!(value_text(142, 4), "Sine");
    }

    /// A lone carrier at ratio 1 plays the note; Transpose and the wheel move it.
    #[test]
    fn the_note_sets_the_pitch() {
        let mut values: Vec<f32> = params().iter().map(|p| p.default).collect();
        // The modulator off: a sine.
        values[4 * OP_PARAMS + OP_OUTPUT_LEVEL] = 0.0;
        // The carrier held at full level.
        let c = 5 * OP_PARAMS;
        values[c..c + 8].copy_from_slice(&[99.0, 99.0, 99.0, 99.0, 99.0, 99.0, 99.0, 0.0]);
        for rate in [48000u32, 44100] {
            for (note, hz) in [(57u8, 220.0f64), (69, 440.0)] {
                let Some(mut inst) = open_native_instrument(ID, rate, &values) else {
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
        let mut up = values.clone();
        up[TRANSPOSE] = 36.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &up) else {
            panic!("does not open")
        };
        let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, 57, 100)]);
        let got = pitch(&l[24000..], 48000.0, 60.0, 1500.0);
        assert!((got / 440.0 - 1.0).abs() < 0.01, "an octave up is {got} Hz");
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        // The wheel all the way up: two semitones.
        let (l, _) = play(
            &mut inst,
            48000,
            256,
            &[(0, 0xe0, 127, 127), (0, 0x90, 57, 100)],
        );
        let got = pitch(&l[24000..], 48000.0, 60.0, 1500.0);
        let want = 220.0 * 2f64.powf(2.0 / 12.0);
        assert!((got / want - 1.0).abs() < 0.01, "bent it is {got} Hz");
    }

    /// The modulator's level changes the tone of the next note.
    #[test]
    fn a_voice_change_is_heard_on_the_next_note() {
        let mut values: Vec<f32> = params().iter().map(|p| p.default).collect();
        // The modulator off: a sine.
        values[4 * OP_PARAMS + OP_OUTPUT_LEVEL] = 0.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        // Movement from sample to sample, for the same level: how bright the note is.
        let rough = |x: &[f32]| x.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / peak(x);
        let (a, _) = play(&mut inst, 12000, 256, &[(0, 0x90, 48, 100)]);
        inst.set_parameter(4 * OP_PARAMS + OP_OUTPUT_LEVEL, 99.0);
        let _ = play(&mut inst, 96000, 256, &[(0, 0x80, 48, 0)]);
        let (c, _) = play(&mut inst, 12000, 256, &[(0, 0x90, 48, 100)]);
        assert!(
            rough(&c) > rough(&a) * 2.0,
            "{} then {}",
            rough(&a),
            rough(&c)
        );
    }

    /// A bank file gives 32 voices whose values the instrument accepts; anything else none.
    #[test]
    fn a_bank_unpacks_to_parameter_values() {
        // A bank made here: each voice's packed bytes count up, in the data range.
        let mut file = vec![0xf0, 0x43, 0x00, 0x09, 0x20, 0x00];
        for v in 0..32u32 {
            for k in 0..FM6_PACKED as u32 {
                file.push(((v * 7 + k * 13) % 100) as u8);
            }
        }
        // Its name: the last ten bytes of the last voice.
        let at = file.len() - 10;
        file[at..].copy_from_slice(b"PRO WAVE  ");
        file.extend([0x00, 0xf7]);
        let voices = bank_voices(&file);
        assert_eq!(voices.len(), 32);
        assert_eq!(voices[31].0, "PRO WAVE");
        for (_, values) in &voices {
            assert_eq!(values.len(), VOICE_PARAMS);
            for (p, v) in params().iter().zip(values) {
                assert!(p.min <= *v && *v <= p.max, "{} is {v}", p.name);
            }
            let Some(mut inst) = open_native_instrument(ID, 48000, values) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 4800, 256, &[(0, 0x90, 60, 100)]);
            assert!(l.iter().all(|x| x.is_finite()));
        }
        // The data alone is read too; a short file, or one with a status byte in it, is not.
        assert_eq!(bank_voices(&file[6..6 + 4096]).len(), 32);
        assert!(bank_voices(&file[..2000]).is_empty());
        assert!(bank_voices(b"not a bank").is_empty());
        file[6 + 5] = 0x90;
        assert_eq!(bank_voices(&file).len(), 31);
    }
}
