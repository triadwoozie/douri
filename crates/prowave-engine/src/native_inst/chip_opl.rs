// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The chip is ymfm's YM3812 in the vendored crate
// crates/chip-dsp (vendor/ymfm: BSD-3-Clause, Copyright (c) 2021 Aaron Giles,
// https://github.com/aaronsgiles/ymfm at commit 81aec25), unchanged; see its NOTICE.

//! Chip FM 2-Op: the nine two-operator FM voices of an OPL2 sound chip, as in the sound
//! cards of the early 1990s.
//!
//! Logic Pro equivalent: none; EFM1 is Logic's two-operator FM synthesizer.
//!
//! One patch for all nine voices: a modulator and a carrier, each with a frequency multiple,
//! a level, a four-stage envelope and one of the chip's four waveforms, joined by FM or added,
//! with feedback on the modulator. The numbers are the chip's own register values, except that
//! Level and Sustain run upward (the chip stores attenuation). Velocity turns the carrier
//! down (and, when the operators are added, the modulator with it).

use super::chip::{block_fnum, hz, Board, ChipInst};
use super::NativeInst;
use crate::native_fx::NativeParam;
use chip_dsp::{Chip, CHIP_OPL2};

pub const ID: &str = "prowave:chip-opl";
pub const NAME: &str = "Chip FM 2-Op";

pub const ALGORITHM: usize = 0;
pub const FEEDBACK: usize = 1;
/// The modulator's parameters start here, the carrier's `OP_PARAMS` later: Mult, Level,
/// Attack, Decay, Sustain, Release, Wave.
pub const MOD: usize = 2;
pub const OP_PARAMS: usize = 7;
pub const CAR: usize = MOD + OP_PARAMS;
pub const TUNE: usize = 16;
pub const VOICES: usize = 17;
pub const LEVEL: usize = 18;
pub const COUNT: usize = 19;

static WAVES: [&str; 4] = ["Sine", "Half Sine", "Absolute Sine", "Quarter Sine"];

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::choice("Algorithm", 0, 0, &["FM", "Additive"]),
    NativeParam::knob("Feedback", 0, 7, 4, ""),
    NativeParam::knob("Mod Mult", 0, 15, 2, ""),
    NativeParam::knob("Mod Level", 0, 63, 42, ""),
    NativeParam::knob("Mod Attack", 0, 15, 15, ""),
    NativeParam::knob("Mod Decay", 0, 15, 5, ""),
    NativeParam::knob("Mod Sustain", 0, 15, 9, ""),
    NativeParam::knob("Mod Release", 0, 15, 6, ""),
    NativeParam::choice("Mod Wave", 0, 0, &WAVES),
    NativeParam::knob("Car Mult", 0, 15, 1, ""),
    NativeParam::knob("Car Level", 0, 63, 63, ""),
    NativeParam::knob("Car Attack", 0, 15, 15, ""),
    NativeParam::knob("Car Decay", 0, 15, 3, ""),
    NativeParam::knob("Car Sustain", 0, 15, 11, ""),
    NativeParam::knob("Car Release", 0, 15, 7, ""),
    NativeParam::choice("Car Wave", 0, 0, &WAVES),
    NativeParam::knob("Tune", -24, 24, 0, "st"),
    NativeParam::knob("Voices", 1, 9, 9, ""),
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
        _ => v.to_string(),
    }
}

/// The register offset of each voice's modulator; its carrier is 3 further on.
const MOD_SLOT: [u32; 9] = [0, 1, 2, 8, 9, 10, 16, 17, 18];
/// Steps of 0.75 dB a key at the lowest velocity is turned down by.
const VELOCITY_STEPS: f32 = 28.0;

#[derive(Default)]
pub(crate) struct Opl;

impl Opl {
    /// An operator's total level (attenuation, 0 loudest) for its Level and the velocity.
    fn total_level(level: i32, velocity: Option<f32>) -> u8 {
        let down = velocity.map_or(0.0, |v| (1.0 - v.clamp(0.0, 1.0)) * VELOCITY_STEPS);
        ((63 - level.clamp(0, 63)) as f32 + down)
            .round()
            .clamp(0.0, 63.0) as u8
    }

    fn frequency(chip: &mut Chip, ch: usize, pitch: f32, on: bool) {
        let (block, fnum) = block_fnum(hz(pitch), chip.rate() as f32, 20, 10, 8);
        chip.write(0xa0 + ch as u32, (fnum & 0xff) as u8);
        let key = if on { 0x20 } else { 0 };
        chip.write(
            0xb0 + ch as u32,
            key | ((block as u8) << 2) | (fnum >> 8) as u8,
        );
    }
}

impl Board for Opl {
    const KIND: i32 = CHIP_OPL2;
    const VOICES: usize = 9;
    const TUNE: usize = TUNE;
    const POLY: usize = VOICES;
    const LEVEL: usize = LEVEL;

    fn params() -> &'static [NativeParam] {
        &PARAMS
    }

    fn program(&mut self, chip: &mut Chip, p: &[i32]) {
        // The waveform registers work.
        chip.write(0x01, 0x20);
        let nib = |v: i32| v.clamp(0, 15) as u8;
        for (ch, slot) in MOD_SLOT.iter().enumerate() {
            for (base, slot) in [(MOD, *slot), (CAR, *slot + 3)] {
                // Sustained (the envelope holds at its sustain level while the key is down).
                chip.write(0x20 + slot, 0x20 | nib(p[base]));
                chip.write(0x60 + slot, (nib(p[base + 2]) << 4) | nib(p[base + 3]));
                chip.write(
                    0x80 + slot,
                    ((15 - nib(p[base + 4])) << 4) | nib(p[base + 5]),
                );
                chip.write(0xe0 + slot, p[base + 6].clamp(0, 3) as u8);
            }
            chip.write(0x40 + *slot, Self::total_level(p[MOD + 1], None));
            chip.write(0x40 + *slot + 3, Self::total_level(p[CAR + 1], None));
            let algorithm = u8::from(p[ALGORITHM] != 0);
            chip.write(
                0xc0 + ch as u32,
                ((p[FEEDBACK].clamp(0, 7) as u8) << 1) | algorithm,
            );
        }
    }

    fn key_on(&mut self, chip: &mut Chip, ch: usize, pitch: f32, velocity: f32, p: &[i32]) {
        let slot = MOD_SLOT[ch.min(8)];
        // Added, both operators are heard: both follow the velocity.
        let modulator = (p[ALGORITHM] != 0).then_some(velocity);
        chip.write(0x40 + slot, Self::total_level(p[MOD + 1], modulator));
        chip.write(
            0x40 + slot + 3,
            Self::total_level(p[CAR + 1], Some(velocity)),
        );
        Self::frequency(chip, ch, pitch, true);
    }

    fn key_off(&mut self, chip: &mut Chip, ch: usize, pitch: f32, _p: &[i32]) {
        Self::frequency(chip, ch, pitch, false);
    }

    fn retune(&mut self, chip: &mut Chip, ch: usize, pitch: f32, held: bool, _p: &[i32]) {
        Self::frequency(chip, ch, pitch, held);
    }
}

pub(crate) fn open(rate: u32, values: &[f32]) -> Box<dyn NativeInst> {
    Box::new(ChipInst::<Opl>::new(rate, values))
}

#[cfg(test)]
mod tests {
    use super::super::test_notes::{peak, pitch, play, sounds_and_falls_silent};
    use super::super::{open_native_instrument, BlockProcessor};
    use super::*;

    #[test]
    fn chip_opl_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 4.0);
        sounds_and_falls_silent(ID, 44100, &[], 4.0);
    }

    #[test]
    fn the_note_sets_the_pitch() {
        // Added, the modulator silent: the carrier's sine alone.
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[ALGORITHM] = 1.0;
        values[MOD + 1] = 0.0;
        values[CAR + 4] = 15.0;
        for (note, want) in [(57u8, 220.0f64), (69, 440.0), (33, 55.0)] {
            let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 100)]);
            let got = pitch(&l[24000..], 48000.0, 40.0, 1500.0);
            assert!((got / want - 1.0).abs() < 0.01, "note {note} is {got} Hz");
        }
        // Tune and the wheel move a held note.
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        let _ = play(&mut inst, 4800, 256, &[(0, 0x90, 57, 100)]);
        inst.set_parameter(TUNE, 12.0);
        let (l, _) = play(&mut inst, 48000, 256, &[]);
        let got = pitch(&l[24000..], 48000.0, 40.0, 1500.0);
        assert!((got / 440.0 - 1.0).abs() < 0.01, "tuned up it is {got} Hz");
    }

    /// A softer key is quieter, and the modulator makes the tone brighter.
    #[test]
    fn velocity_and_the_modulator_are_heard() {
        let run = |values: &[f32], velocity: u8| {
            let Some(mut inst) = open_native_instrument(ID, 48000, values) else {
                panic!("does not open")
            };
            play(&mut inst, 24000, 256, &[(0, 0x90, 48, velocity)]).0
        };
        let loud = run(&[], 127);
        let soft = run(&[], 30);
        assert!(
            peak(&soft) < peak(&loud) * 0.5,
            "{} {}",
            peak(&soft),
            peak(&loud)
        );
        let mut plain: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        plain[MOD + 1] = 0.0;
        let rough = |x: &[f32]| x.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / peak(x);
        assert!(rough(&loud) > rough(&run(&plain, 127)) * 1.5);
    }
}
