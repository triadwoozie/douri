// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The chip is Tetorica's Sega PSG in the vendored
// crate crates/chip-dsp (vendor/tetorica_sega_psg: BSD-3-Clause, Copyright (c) 2026 kyorohiro,
// https://github.com/kyorohiro/tetorica-soundchip-native at commit b4f2543), unchanged; see
// its NOTICE.

//! Chip PSG: the three square-wave voices of a programmable sound generator, as in 8-bit
//! consoles and home computers.
//!
//! Logic Pro equivalent: none; a square wave in Retro Synth or ES1 is the nearest.
//!
//! The chip has a tone period and a volume of sixteen steps of 2 dB for each voice, and
//! nothing else: the envelope here is Pro Wave's ([`Adsr`]), written to the volume register
//! every 32 frames, in the chip's steps, as a game's sound driver did. A note too low for the
//! chip's ten-bit period (below about 110 Hz) sounds octaves up.

use super::chip::{hz, Board, ChipInst};
use super::envelope::{Adsr, AdsrRates, AdsrTimes};
use super::NativeInst;
use crate::native_fx::NativeParam;
use chip_dsp::{Chip, CHIP_PSG};

pub const ID: &str = "prowave:chip-psg";
pub const NAME: &str = "Chip PSG";

pub const ATTACK: usize = 0;
pub const DECAY: usize = 1;
pub const SUSTAIN: usize = 2;
pub const RELEASE: usize = 3;
pub const TUNE: usize = 4;
pub const VOICES: usize = 5;
pub const LEVEL: usize = 6;
pub const COUNT: usize = 7;

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::knob("Attack", 0, 2000, 2, "ms"),
    NativeParam::knob("Decay", 0, 5000, 300, "ms"),
    NativeParam::knob("Sustain", 0, 100, 60, "%"),
    NativeParam::knob("Release", 0, 5000, 200, "ms"),
    NativeParam::knob("Tune", -24, 24, 0, "st"),
    NativeParam::knob("Voices", 1, 3, 3, ""),
    NativeParam::knob("Level", -48, 12, -6, "dB"),
];

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        ATTACK | DECAY | RELEASE => format!("{v}ms"),
        SUSTAIN => format!("{v}%"),
        TUNE => format!("{v}st"),
        VOICES => v.to_string(),
        LEVEL => format!("{v}dB"),
        _ => String::new(),
    }
}

/// The chip's clock: a tone's frequency is this over 32 times its period.
const CLOCK: f32 = 3_579_545.0;

/// The chip's attenuation (0 full, 15 silent, 2 dB a step) for a level of 0 to 1.
pub(crate) fn attenuation(level: f32) -> u8 {
    if level <= 0.035 {
        return 15;
    }
    (-10.0 * level.min(1.0).log10()).round().clamp(0.0, 14.0) as u8
}

/// The chip's ten-bit period for a frequency, octaves up when it is too low to fit.
pub(crate) fn period(hz: f32) -> u16 {
    let mut n = CLOCK / (32.0 * hz.max(1.0));
    while n > 1023.0 {
        n /= 2.0;
    }
    n.round().clamp(1.0, 1023.0) as u16
}

#[derive(Default)]
pub(crate) struct Psg {
    env: [Adsr; 3],
    velocity: [f32; 3],
    /// The volume last written to each voice.
    written: [Option<u8>; 3],
}

impl Psg {
    fn frequency(chip: &mut Chip, ch: usize, pitch: f32) {
        let n = period(hz(pitch));
        let ch = ch.min(2) as u8;
        // The latch byte with the low four bits, then the high six.
        chip.write(0, 0x80 | (ch << 5) | (n & 0x0f) as u8);
        chip.write(0, ((n >> 4) & 0x3f) as u8);
    }

    fn volume(&mut self, chip: &mut Chip, ch: usize, level: f32) {
        let att = attenuation(level);
        if self.written[ch] != Some(att) {
            self.written[ch] = Some(att);
            chip.write(0, 0x90 | ((ch as u8) << 5) | att);
        }
    }
}

impl Board for Psg {
    const KIND: i32 = CHIP_PSG;
    const VOICES: usize = 3;
    const TUNE: usize = TUNE;
    const POLY: usize = VOICES;
    const LEVEL: usize = LEVEL;

    fn params() -> &'static [NativeParam] {
        &PARAMS
    }

    fn program(&mut self, chip: &mut Chip, _p: &[i32]) {
        // The noise voice silent; the tone voices as their envelopes have them.
        chip.write(0, 0xff);
        self.written = [None; 3];
        for ch in 0..3 {
            let level = self.env[ch].level() * self.velocity[ch];
            self.volume(chip, ch, level);
        }
    }

    fn key_on(&mut self, chip: &mut Chip, ch: usize, pitch: f32, velocity: f32, _p: &[i32]) {
        let ch = ch.min(2);
        Self::frequency(chip, ch, pitch);
        self.velocity[ch] = velocity;
        self.env[ch].gate_on();
    }

    fn key_off(&mut self, _chip: &mut Chip, ch: usize, _pitch: f32, _p: &[i32]) {
        self.env[ch.min(2)].gate_off();
    }

    fn retune(&mut self, chip: &mut Chip, ch: usize, pitch: f32, _held: bool, _p: &[i32]) {
        Self::frequency(chip, ch, pitch);
    }

    fn tick(&mut self, chip: &mut Chip, frames: usize, p: &[i32]) {
        // The envelopes move once a block: the block rate is their sample rate.
        let rate = chip.rate() as f32 / frames.max(1) as f32;
        let rates = AdsrRates::new(
            &AdsrTimes {
                attack_s: p[ATTACK] as f32 * 0.001,
                decay_s: p[DECAY] as f32 * 0.001,
                sustain: p[SUSTAIN] as f32 / 100.0,
                release_s: p[RELEASE] as f32 * 0.001,
            },
            rate,
        );
        for ch in 0..3 {
            let level = rates.tick(&mut self.env[ch]) * self.velocity[ch];
            self.volume(chip, ch, level);
        }
    }
}

pub(crate) fn open(rate: u32, values: &[f32]) -> Box<dyn NativeInst> {
    Box::new(ChipInst::<Psg>::new(rate, values))
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, pitch, play, sounds_and_falls_silent};
    use super::*;

    #[test]
    fn chip_psg_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 3.0);
        sounds_and_falls_silent(ID, 44100, &[], 3.0);
    }

    #[test]
    fn periods_and_volumes_are_the_chips() {
        // 440 Hz: 3579545 / (32 * 440) = 254.2.
        assert_eq!(period(440.0), 254);
        // 55 Hz does not fit in ten bits: it plays at 110 Hz.
        assert_eq!(period(55.0), period(110.0));
        assert_eq!(period(110.0), 1017);
        assert_eq!(period(1.0e6), 1);
        assert_eq!(
            [1.0, 0.794, 0.1, 0.04, 0.03, 0.0].map(attenuation),
            [0, 1, 10, 14, 15, 15]
        );
    }

    #[test]
    fn the_note_sets_the_pitch_and_velocity_the_level() {
        for (note, want) in [(57u8, 220.0f64), (69, 440.0), (81, 880.0)] {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 100)]);
            let got = pitch(&l[24000..], 48000.0, 60.0, 1500.0);
            // The period is a whole number: within a few cents.
            assert!((got / want - 1.0).abs() < 0.01, "note {note} is {got} Hz");
        }
        let run = |velocity: u8| {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
                panic!("does not open")
            };
            peak(&play(&mut inst, 12000, 256, &[(0, 0x90, 69, velocity)]).0[..6000])
        };
        assert!(run(20) < run(127) * 0.4, "{} {}", run(20), run(127));
    }
}
