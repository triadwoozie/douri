// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The chip is ymfm's YM2413 in the vendored crate
// crates/chip-dsp (vendor/ymfm: BSD-3-Clause, Copyright (c) 2021 Aaron Giles,
// https://github.com/aaronsgiles/ymfm at commit 81aec25), unchanged; see its NOTICE.

//! Chip FM Preset: the nine voices of an OPLL sound chip, which plays fifteen instruments
//! built into it.
//!
//! Logic Pro equivalent: none.
//!
//! The chip's instruments cannot be edited: a voice has an instrument number, a volume in
//! sixteen steps of 3 dB (the key's velocity here) and a sustain bit that lengthens the
//! release. The instrument tables are ymfm's (it says where they come from).

use super::chip::{block_fnum, hz, Board, ChipInst};
use super::NativeInst;
use crate::native_fx::NativeParam;
use chip_dsp::{Chip, CHIP_OPLL};

pub const ID: &str = "prowave:chip-opll";
pub const NAME: &str = "Chip FM Preset";

pub const INSTRUMENT: usize = 0;
pub const SUSTAIN: usize = 1;
pub const TUNE: usize = 2;
pub const VOICES: usize = 3;
pub const LEVEL: usize = 4;
pub const COUNT: usize = 5;

/// The chip's instruments 1 to 15, by the names of its data sheet.
pub static INSTRUMENTS: [&str; 15] = [
    "Violin",
    "Guitar",
    "Piano",
    "Flute",
    "Clarinet",
    "Oboe",
    "Trumpet",
    "Organ",
    "Horn",
    "Synthesizer",
    "Harpsichord",
    "Vibraphone",
    "Synth Bass",
    "Acoustic Bass",
    "Electric Guitar",
];

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::choice("Instrument", 0, 2, &INSTRUMENTS),
    NativeParam::choice("Sustain", 0, 0, &["Off", "On"]),
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

/// Volume steps (3 dB each) a key at the lowest velocity is turned down by.
const VELOCITY_STEPS: f32 = 8.0;

#[derive(Default)]
pub(crate) struct Opll;

impl Opll {
    fn frequency(chip: &mut Chip, ch: usize, pitch: f32, on: bool, p: &[i32]) {
        let (block, fnum) = block_fnum(hz(pitch), chip.rate() as f32, 19, 9, 8);
        chip.write(0x10 + ch as u32, (fnum & 0xff) as u8);
        let sustain = if p[SUSTAIN] != 0 { 0x20 } else { 0 };
        let key = if on { 0x10 } else { 0 };
        chip.write(
            0x20 + ch as u32,
            sustain | key | ((block as u8) << 1) | (fnum >> 8) as u8,
        );
    }

    fn voice(chip: &mut Chip, ch: usize, velocity: f32, p: &[i32]) {
        let down = ((1.0 - velocity.clamp(0.0, 1.0)) * VELOCITY_STEPS).round() as u8;
        let instrument = (p[INSTRUMENT].clamp(0, 14) + 1) as u8;
        chip.write(0x30 + ch as u32, (instrument << 4) | down.min(15));
    }
}

impl Board for Opll {
    const KIND: i32 = CHIP_OPLL;
    const VOICES: usize = 9;
    const TUNE: usize = TUNE;
    const POLY: usize = VOICES;
    const LEVEL: usize = LEVEL;

    fn params() -> &'static [NativeParam] {
        &PARAMS
    }

    fn program(&mut self, chip: &mut Chip, p: &[i32]) {
        // Nine melody voices: the rhythm section off.
        chip.write(0x0e, 0);
        for ch in 0..9 {
            Self::voice(chip, ch, 1.0, p);
        }
    }

    fn key_on(&mut self, chip: &mut Chip, ch: usize, pitch: f32, velocity: f32, p: &[i32]) {
        Self::voice(chip, ch, velocity, p);
        Self::frequency(chip, ch, pitch, true, p);
    }

    fn key_off(&mut self, chip: &mut Chip, ch: usize, pitch: f32, p: &[i32]) {
        Self::frequency(chip, ch, pitch, false, p);
    }

    fn retune(&mut self, chip: &mut Chip, ch: usize, pitch: f32, held: bool, p: &[i32]) {
        Self::frequency(chip, ch, pitch, held, p);
    }
}

pub(crate) fn open(rate: u32, values: &[f32]) -> Box<dyn NativeInst> {
    Box::new(ChipInst::<Opll>::new(rate, values))
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, play, sounds_and_falls_silent, tuned_to};
    use super::*;

    #[test]
    fn chip_opll_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 4.0);
        sounds_and_falls_silent(ID, 44100, &[], 4.0);
    }

    /// Every instrument sounds at the note, and they do not all sound alike.
    #[test]
    fn every_instrument_plays_the_note() {
        let mut peaks = Vec::new();
        for (i, name) in INSTRUMENTS.iter().enumerate() {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[i as f32]) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 24000, 256, &[(0, 0x90, 57, 110)]);
            assert!(peak(&l) > 0.01, "{name}: {}", peak(&l));
            // Tuned to the note, or an octave under it (an instrument whose carrier runs at
            // half the frequency).
            let ratio = tuned_to(&l[2400..14400], 220.0, 48000.0).max(tuned_to(
                &l[2400..14400],
                110.0,
                48000.0,
            ));
            assert!(ratio > 5.0, "{name}: {ratio}");
            peaks.push((peak(&l[12000..]) * 1000.0) as i32);
        }
        peaks.sort_unstable();
        peaks.dedup();
        assert!(peaks.len() > 5, "{peaks:?}");
    }

    /// A softer key is quieter, in the chip's 3 dB steps.
    #[test]
    fn velocity_is_the_chips_volume() {
        let run = |velocity: u8| {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[7.0]) else {
                panic!("does not open")
            };
            peak(&play(&mut inst, 24000, 256, &[(0, 0x90, 57, velocity)]).0)
        };
        let (loud, soft) = (run(127), run(1));
        assert!(soft > 0.0 && soft < loud * 0.12, "{soft} {loud}");
    }
}
