// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The chip is ymfm's YM2612 in the vendored crate
// crates/chip-dsp (vendor/ymfm: BSD-3-Clause, Copyright (c) 2021 Aaron Giles,
// https://github.com/aaronsgiles/ymfm at commit 81aec25), unchanged; see its NOTICE.

//! Chip FM 4-Op: the six four-operator FM voices of an OPN2 sound chip, as in a 16-bit
//! console of 1988.
//!
//! Logic Pro equivalent: none; Retro Synth's FM engine is the nearest.
//!
//! One patch for all six voices: four operators, each with a frequency multiple, a detune, a
//! level and the chip's envelope (attack, decay to the sustain level, a second decay while
//! the key is held, release), joined by one of the chip's eight algorithms, with feedback on
//! operator 1. The numbers are the chip's own register values, except that Level and Sustain
//! run upward (the chip stores attenuation). Velocity turns down the operators the algorithm
//! sends to the output.

use super::chip::{block_fnum, hz, Board, ChipInst};
use super::NativeInst;
use crate::native_fx::NativeParam;
use chip_dsp::{Chip, CHIP_OPN2};
use std::sync::OnceLock;

pub const ID: &str = "prowave:chip-opn";
pub const NAME: &str = "Chip FM 4-Op";

pub const ALGORITHM: usize = 0;
pub const FEEDBACK: usize = 1;
/// Operator `n` (1 to 4) starts at `OPS + (n - 1) * OP_PARAMS`: Mult, Level, Attack, Decay,
/// Sustain Rate, Sustain, Release, Detune.
pub const OPS: usize = 2;
pub const OP_PARAMS: usize = 8;
pub const TUNE: usize = 34;
pub const VOICES: usize = 35;
pub const LEVEL: usize = 36;
pub const COUNT: usize = 37;

/// One operator's parameters: (name, maximum).
static OP_TABLE: [(&str, i32); OP_PARAMS] = [
    ("Mult", 15),
    ("Level", 127),
    ("Attack", 31),
    ("Decay", 31),
    ("Sustain Rate", 31),
    ("Sustain", 15),
    ("Release", 15),
    ("Detune", 7),
];

/// The default patch's operators (Pro Wave's own): two modulators, each into its carrier.
static OP_DEFAULTS: [[i32; OP_PARAMS]; 4] = [
    [2, 96, 31, 10, 4, 10, 6, 0],
    [1, 127, 31, 8, 2, 12, 7, 0],
    [1, 100, 31, 12, 5, 8, 6, 3],
    [1, 120, 28, 8, 2, 12, 7, 0],
];

pub fn params() -> &'static [NativeParam] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    static PARAMS: OnceLock<Vec<NativeParam>> = OnceLock::new();
    PARAMS.get_or_init(|| {
        let names = NAMES.get_or_init(|| {
            (0..4 * OP_PARAMS)
                .map(|k| format!("Op {} {}", k / OP_PARAMS + 1, OP_TABLE[k % OP_PARAMS].0))
                .collect()
        });
        let mut out = vec![
            NativeParam::knob("Algorithm", 0, 7, 4, ""),
            NativeParam::knob("Feedback", 0, 7, 3, ""),
        ];
        for (k, name) in names.iter().enumerate() {
            let (op, which) = (k / OP_PARAMS, k % OP_PARAMS);
            out.push(NativeParam::knob(
                name.as_str(),
                0,
                OP_TABLE[which].1,
                OP_DEFAULTS[op][which],
                "",
            ));
        }
        out.push(NativeParam::knob("Tune", -24, 24, 0, "st"));
        out.push(NativeParam::knob("Voices", 1, 6, 6, ""));
        out.push(NativeParam::knob("Level", -48, 12, -6, "dB"));
        out
    })
}

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        TUNE => format!("{v}st"),
        LEVEL => format!("{v}dB"),
        _ if k < COUNT => v.to_string(),
        _ => String::new(),
    }
}

/// Whether operator `op` (0 to 3) of `algorithm` goes to the output.
pub(crate) fn is_carrier(algorithm: i32, op: usize) -> bool {
    match algorithm {
        0..=3 => op == 3,
        4 => op == 1 || op == 3,
        5 | 6 => op >= 1,
        _ => true,
    }
}

/// The register offset of each operator: the chip keeps them in the order 1, 3, 2, 4.
const SLOT: [u32; 4] = [0, 8, 4, 12];
/// Steps of 0.75 dB a key at the lowest velocity is turned down by.
const VELOCITY_STEPS: f32 = 28.0;

#[derive(Default)]
pub(crate) struct Opn;

impl Opn {
    /// The port bit and the voice's number within its port.
    fn port(ch: usize) -> (u32, u32) {
        let ch = ch.min(5) as u32;
        ((ch / 3) << 8, ch % 3)
    }

    fn total_level(level: i32, velocity: Option<f32>) -> u8 {
        let down = velocity.map_or(0.0, |v| (1.0 - v.clamp(0.0, 1.0)) * VELOCITY_STEPS);
        ((127 - level.clamp(0, 127)) as f32 + down)
            .round()
            .clamp(0.0, 127.0) as u8
    }

    fn levels(chip: &mut Chip, ch: usize, velocity: Option<f32>, p: &[i32]) {
        let (port, n) = Self::port(ch);
        for (op, slot) in SLOT.iter().enumerate() {
            let heard = velocity.filter(|_| is_carrier(p[ALGORITHM], op));
            let level = p[OPS + op * OP_PARAMS + 1];
            chip.write(port | (0x40 + slot + n), Self::total_level(level, heard));
        }
    }

    fn frequency(chip: &mut Chip, ch: usize, pitch: f32) {
        let (port, n) = Self::port(ch);
        let (block, fnum) = block_fnum(hz(pitch), chip.rate() as f32, 21, 11, 8);
        // The high part first: the chip takes both when the low part is written.
        chip.write(port | (0xa4 + n), ((block as u8) << 3) | (fnum >> 8) as u8);
        chip.write(port | (0xa0 + n), (fnum & 0xff) as u8);
    }

    fn key(chip: &mut Chip, ch: usize, on: bool) {
        let (port, n) = Self::port(ch);
        let voice = (n + if port != 0 { 4 } else { 0 }) as u8;
        chip.write(0x28, if on { 0xf0 | voice } else { voice });
    }
}

impl Board for Opn {
    const KIND: i32 = CHIP_OPN2;
    const VOICES: usize = 6;
    const TUNE: usize = TUNE;
    const POLY: usize = VOICES;
    const LEVEL: usize = LEVEL;

    fn params() -> &'static [NativeParam] {
        params()
    }

    fn program(&mut self, chip: &mut Chip, p: &[i32]) {
        // No LFO, voice 3 as the others, no DAC in place of voice 6.
        chip.write(0x22, 0);
        chip.write(0x27, 0);
        chip.write(0x2b, 0);
        for ch in 0..6 {
            let (port, n) = Self::port(ch);
            for (op, slot) in SLOT.iter().enumerate() {
                let o = &p[OPS + op * OP_PARAMS..OPS + (op + 1) * OP_PARAMS];
                let r = port | (slot + n);
                chip.write(
                    0x30 + r,
                    ((o[7].clamp(0, 7) as u8) << 4) | o[0].clamp(0, 15) as u8,
                );
                chip.write(0x50 + r, o[2].clamp(0, 31) as u8);
                chip.write(0x60 + r, o[3].clamp(0, 31) as u8);
                chip.write(0x70 + r, o[4].clamp(0, 31) as u8);
                let sustain = (15 - o[5].clamp(0, 15)) as u8;
                chip.write(0x80 + r, (sustain << 4) | o[6].clamp(0, 15) as u8);
                chip.write(0x90 + r, 0);
            }
            Self::levels(chip, ch, None, p);
            let feedback = (p[FEEDBACK].clamp(0, 7) as u8) << 3;
            chip.write(port | (0xb0 + n), feedback | p[ALGORITHM].clamp(0, 7) as u8);
            // Both speakers.
            chip.write(port | (0xb4 + n), 0xc0);
        }
    }

    fn key_on(&mut self, chip: &mut Chip, ch: usize, pitch: f32, velocity: f32, p: &[i32]) {
        Self::levels(chip, ch, Some(velocity), p);
        Self::frequency(chip, ch, pitch);
        Self::key(chip, ch, true);
    }

    fn key_off(&mut self, chip: &mut Chip, ch: usize, _pitch: f32, _p: &[i32]) {
        Self::key(chip, ch, false);
    }

    fn retune(&mut self, chip: &mut Chip, ch: usize, pitch: f32, _held: bool, _p: &[i32]) {
        Self::frequency(chip, ch, pitch);
    }
}

pub(crate) fn open(rate: u32, values: &[f32]) -> Box<dyn NativeInst> {
    Box::new(ChipInst::<Opn>::new(rate, values))
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{pitch, play, sounds_and_falls_silent};
    use super::*;

    #[test]
    fn chip_opn_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 4.0);
        sounds_and_falls_silent(ID, 44100, &[], 4.0);
    }

    #[test]
    fn the_table_and_the_carriers_are_the_chips() {
        let p = params();
        assert_eq!(p.len(), COUNT);
        assert_eq!(p[OPS].name, "Op 1 Mult");
        assert_eq!(p[OPS + 3 * OP_PARAMS + 7].name, "Op 4 Detune");
        assert_eq!(p[TUNE].name, "Tune");
        let carriers = |a: i32| (0..4).filter(|op| is_carrier(a, *op)).count();
        assert_eq!([0, 3, 4, 5, 6, 7].map(carriers), [1, 1, 2, 3, 3, 4]);
    }

    #[test]
    fn the_note_sets_the_pitch() {
        // Algorithm 7 (every operator to the output) with only operator 4 up: a sine.
        let mut values: Vec<f32> = params().iter().map(|p| p.default).collect();
        values[ALGORITHM] = 7.0;
        values[FEEDBACK] = 0.0;
        for op in 0..3 {
            values[OPS + op * OP_PARAMS + 1] = 0.0;
        }
        let o4 = OPS + 3 * OP_PARAMS;
        values[o4 + 3] = 0.0;
        values[o4 + 4] = 0.0;
        values[o4 + 5] = 15.0;
        for (note, want) in [(57u8, 220.0f64), (69, 440.0), (33, 55.0)] {
            let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                panic!("does not open")
            };
            let (l, r) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 100)]);
            assert_eq!(l, r);
            let got = pitch(&l[24000..], 48000.0, 40.0, 1500.0);
            assert!((got / want - 1.0).abs() < 0.01, "note {note} is {got} Hz");
        }
    }

    /// All six voices sound (both ports), and a softer key is quieter.
    #[test]
    fn six_voices_and_velocity() {
        let level = |notes: &[(usize, u8, u8, u8)]| {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 24000, 256, notes);
            (l.iter().map(|v| v * v).sum::<f32>() / l.len() as f32).sqrt()
        };
        let one = level(&[(0, 0x90, 40, 100)]);
        let six: Vec<(usize, u8, u8, u8)> =
            (0..6).map(|k| (0, 0x90, 40 + 5 * k as u8, 100)).collect();
        assert!(level(&six) > one * 1.8, "{} {}", level(&six), one);
        assert!(level(&six[..3]) < level(&six) * 0.9);
        assert!(level(&[(0, 0x90, 40, 20)]) < one * 0.5);
    }
}
