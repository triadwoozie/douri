// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The chip is the chips project's m6581.h in the
// vendored crate crates/chip-dsp (vendor/chips-m6581: zlib licence, Copyright (c) 2018 Andre
// Weissflog, based on tedplay (c) 2012 Attila Grosz; https://github.com/floooh/chips at
// commit 9e88298), unchanged; see its NOTICE.

//! Chip 6581: the three voices and the filter of the sound chip of an 8-bit home computer
//! of 1982.
//!
//! Logic Pro equivalent: none.
//!
//! One patch for the three voices: a waveform (with the pulse's width), the chip's envelope
//! (four numbers of 0 to 15), and the chip's one filter, which all three voices pass through
//! when it is on. The chip has no volume per voice, so velocity is not heard.

use super::chip::{hz, Board, ChipInst};
use super::NativeInst;
use crate::native_fx::NativeParam;
use chip_dsp::{Chip, CHIP_SID};

pub const ID: &str = "prowave:chip-sid";
pub const NAME: &str = "Chip 6581";

pub const WAVE: usize = 0;
pub const PULSE_WIDTH: usize = 1;
pub const ATTACK: usize = 2;
pub const DECAY: usize = 3;
pub const SUSTAIN: usize = 4;
pub const RELEASE: usize = 5;
pub const FILTER: usize = 6;
pub const CUTOFF: usize = 7;
pub const RESONANCE: usize = 8;
pub const TUNE: usize = 9;
pub const VOICES: usize = 10;
pub const LEVEL: usize = 11;
pub const COUNT: usize = 12;

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::choice("Wave", 0, 1, &["Triangle", "Sawtooth", "Pulse", "Noise"]),
    NativeParam::knob("Pulse Width", 0, 100, 50, "%"),
    NativeParam::knob("Attack", 0, 15, 1, ""),
    NativeParam::knob("Decay", 0, 15, 8, ""),
    NativeParam::knob("Sustain", 0, 15, 10, ""),
    NativeParam::knob("Release", 0, 15, 8, ""),
    NativeParam::choice(
        "Filter",
        0,
        0,
        &["Off", "Low Pass", "Band Pass", "High Pass"],
    ),
    NativeParam::knob("Cutoff", 0, 100, 60, "%"),
    NativeParam::knob("Resonance", 0, 15, 6, ""),
    NativeParam::knob("Tune", -24, 24, 0, "st"),
    NativeParam::knob("Voices", 1, 3, 3, ""),
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
        PULSE_WIDTH | CUTOFF => format!("{v}%"),
        TUNE => format!("{v}st"),
        LEVEL => format!("{v}dB"),
        _ => v.to_string(),
    }
}

/// The chip's clock in a PAL machine: its frequency registers count in 2^-24ths of it.
const CLOCK: f64 = 985_248.0;

#[derive(Default)]
pub(crate) struct Sid;

impl Sid {
    /// The control register's waveform bit.
    fn wave(p: &[i32]) -> u8 {
        0x10 << p[WAVE].clamp(0, 3)
    }

    fn frequency(chip: &mut Chip, ch: usize, pitch: f32) {
        let f = (f64::from(hz(pitch)) * 16_777_216.0 / CLOCK)
            .round()
            .clamp(0.0, 65535.0) as u32;
        let base = ch.min(2) as u32 * 7;
        chip.write(base, (f & 0xff) as u8);
        chip.write(base + 1, (f >> 8) as u8);
    }
}

impl Board for Sid {
    const KIND: i32 = CHIP_SID;
    const VOICES: usize = 3;
    const TUNE: usize = TUNE;
    const POLY: usize = VOICES;
    const LEVEL: usize = LEVEL;

    fn params() -> &'static [NativeParam] {
        &PARAMS
    }

    fn program(&mut self, chip: &mut Chip, p: &[i32]) {
        let nib = |v: i32| v.clamp(0, 15) as u8;
        let width = (p[PULSE_WIDTH].clamp(0, 100) * 4095 / 100) as u32;
        for ch in 0..3u32 {
            let base = ch * 7;
            chip.write(base + 2, (width & 0xff) as u8);
            chip.write(base + 3, (width >> 8) as u8);
            chip.write(base + 5, (nib(p[ATTACK]) << 4) | nib(p[DECAY]));
            chip.write(base + 6, (nib(p[SUSTAIN]) << 4) | nib(p[RELEASE]));
        }
        // The cutoff's eleven bits; resonance and which voices are filtered; the filter's
        // mode and the chip's volume, full.
        let cutoff = (p[CUTOFF].clamp(0, 100) * 2047 / 100) as u32;
        chip.write(0x15, (cutoff & 7) as u8);
        chip.write(0x16, (cutoff >> 3) as u8);
        let filtered = if p[FILTER] != 0 { 0x07 } else { 0 };
        chip.write(0x17, (nib(p[RESONANCE]) << 4) | filtered);
        let mode = match p[FILTER] {
            1 => 0x10,
            2 => 0x20,
            3 => 0x40,
            _ => 0,
        };
        chip.write(0x18, mode | 0x0f);
    }

    fn key_on(&mut self, chip: &mut Chip, ch: usize, pitch: f32, _velocity: f32, p: &[i32]) {
        Self::frequency(chip, ch, pitch);
        chip.write(ch.min(2) as u32 * 7 + 4, Self::wave(p) | 1);
    }

    fn key_off(&mut self, chip: &mut Chip, ch: usize, _pitch: f32, p: &[i32]) {
        chip.write(ch.min(2) as u32 * 7 + 4, Self::wave(p));
    }

    fn retune(&mut self, chip: &mut Chip, ch: usize, pitch: f32, held: bool, p: &[i32]) {
        Self::frequency(chip, ch, pitch);
        // The waveform may have changed with the patch.
        chip.write(ch.min(2) as u32 * 7 + 4, Self::wave(p) | u8::from(held));
    }
}

pub(crate) fn open(rate: u32, values: &[f32]) -> Box<dyn NativeInst> {
    Box::new(ChipInst::<Sid>::new(rate, values))
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, play, sounds_and_falls_silent, tuned_to};
    use super::*;

    #[test]
    fn chip_sid_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 6.0);
        sounds_and_falls_silent(ID, 44100, &[], 6.0);
    }

    /// The three pitched waveforms play the note; noise is not tuned to it.
    #[test]
    fn the_note_sets_the_pitch_of_every_pitched_wave() {
        for wave in 0..4 {
            for (note, want) in [(57u8, 220.0f64), (69, 440.0)] {
                let Some(mut inst) = open_native_instrument(ID, 48000, &[wave as f32]) else {
                    panic!("does not open")
                };
                let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 100)]);
                assert!(peak(&l) > 0.01, "wave {wave}: {}", peak(&l));
                let ratio = tuned_to(&l[12000..36000], want, 48000.0);
                if wave < 3 {
                    assert!(ratio > 5.0, "wave {wave}, note {note}: {ratio}");
                } else {
                    assert!(ratio < 3.0, "noise: {ratio}");
                }
            }
        }
    }

    /// The low-pass filter dulls a sawtooth and the high-pass brightens it; three notes use three voices and a
    /// fourth takes the oldest's.
    #[test]
    fn the_filter_and_the_three_voices() {
        let bright = |filter: f32, cutoff: f32| {
            let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
            values[FILTER] = filter;
            values[CUTOFF] = cutoff;
            let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 24000, 256, &[(0, 0x90, 45, 100)]);
            l[12000..]
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .sum::<f32>()
                / peak(&l[12000..])
        };
        let (low, off, high) = (bright(1.0, 5.0), bright(0.0, 5.0), bright(3.0, 95.0));
        // The chip's filter does not close far: duller below, brighter above, not silent.
        assert!(low < off * 0.95 && high > off * 1.2, "{low} {off} {high}");
        let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
            panic!("does not open")
        };
        let chord = [
            (0, 0x90, 45, 100),
            (0, 0x90, 52, 100),
            (0, 0x90, 57, 100),
            (0, 0x90, 64, 100),
        ];
        let (l, _) = play(&mut inst, 48000, 256, &chord);
        let x = &l[12000..36000];
        // 110 Hz was taken by the fourth note; the other three sound.
        assert!(tuned_to(x, 329.63, 48000.0) > 2.0);
        assert!(tuned_to(x, 110.0, 48000.0) < tuned_to(x, 220.0, 48000.0));
    }
}
