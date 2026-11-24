// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The chips themselves are the vendored crate
// crates/chip-dsp; see its NOTICE and each chip's file here.

//! What the sound-chip instruments share: a chip that is played by writing its registers
//! (`chip_dsp::Chip`), and the driver that turns notes into those writes.
//!
//! Logic Pro equivalent: none (Logic has no chip instruments).
//!
//! A chip instrument is a [`Board`]: its parameter table and how a patch, a key and a pitch
//! become register writes. [`ChipInst`] does the rest for every board: it gives each note one
//! of the chip's own voices ([`Voices`]), writes the patch again when a parameter moved,
//! follows the wheel, runs the chip at its own rate behind the converter ([`FixedRate`]) and
//! stops running it while nothing sounds.

use super::rate::FixedRate;
use super::voices::Voices;
use super::NativeInst;
use crate::native_fx::NativeParam;
use chip_dsp::Chip;

/// Most voices a chip here has.
pub(crate) const MAX_VOICES: usize = 9;
/// Most parameters a board has.
pub(crate) const MAX_PARAMS: usize = 48;
/// Frames the chip renders at a time, at its own rate.
const BLOCK: usize = 32;
/// Blocks below `QUIET` with no key down after which the chip is not run (about 1 s).
const QUIET_BLOCKS: u32 = 1500;
const QUIET: f32 = 1e-5;

/// A note's frequency in Hz; `pitch` is a MIDI note, with fractions.
pub(crate) fn hz(pitch: f32) -> f32 {
    440.0 * ((pitch - 69.0) / 12.0).exp2()
}

/// A frequency as a chip's block (octave) and F-number: the smallest block whose F-number,
/// `hz * 2^(shift - block) / rate`, fits in `bits` bits.
pub(crate) fn block_fnum(hz: f32, rate: f32, shift: i32, bits: u32, blocks: u32) -> (u32, u32) {
    let top = (1u32 << bits) - 1;
    let base = f64::from(hz.max(0.0)) / f64::from(rate.max(1.0));
    for block in 0..blocks {
        let f = (base * f64::from(shift - block as i32).exp2()).round();
        if f <= f64::from(top) {
            return (block, f as u32);
        }
    }
    (blocks.saturating_sub(1), top)
}

/// A chip and how notes become its registers.
pub(crate) trait Board: Default + Send + 'static {
    /// The chip (`chip_dsp::CHIP_*`).
    const KIND: i32;
    /// The chip's voices.
    const VOICES: usize;
    /// The Tune (semitones), Voices and Level (dB) parameters' indices.
    const TUNE: usize;
    const POLY: usize;
    const LEVEL: usize;
    fn params() -> &'static [NativeParam];
    /// Writes the whole patch (after a reset, or when a parameter moved).
    fn program(&mut self, chip: &mut Chip, p: &[i32]);
    /// Starts voice `ch` at `pitch` (a MIDI note with fractions); `velocity` is 0 to 1.
    fn key_on(&mut self, chip: &mut Chip, ch: usize, pitch: f32, velocity: f32, p: &[i32]);
    /// Lets voice `ch` go.
    fn key_off(&mut self, chip: &mut Chip, ch: usize, pitch: f32, p: &[i32]);
    /// Moves voice `ch` to `pitch` without starting it again; `held`: its key is down.
    fn retune(&mut self, chip: &mut Chip, ch: usize, pitch: f32, held: bool, p: &[i32]);
    /// Before each block of `frames` frames at the chip's rate (envelopes a chip lacks).
    fn tick(&mut self, _chip: &mut Chip, _frames: usize, _p: &[i32]) {}
}

/// A chip instrument.
pub(crate) struct ChipInst<B: Board> {
    /// `None` when memory ran out: the instrument is silent.
    chip: Option<Chip>,
    applied: [i32; MAX_PARAMS],
    alloc: Voices<MAX_VOICES>,
    /// The note each voice plays.
    notes: [f32; MAX_VOICES],
    /// The patch must be written again.
    dirty: bool,
    /// The voices' pitches must be written again (the wheel or Tune moved).
    retune: bool,
    bend: f32,
    quiet: u32,
    asleep: bool,
    /// The output's DC blocker (a chip's DAC can rest away from zero, as its board's output
    /// capacitor expects): the last input and output of each channel, and its coefficient.
    dc: [f32; 4],
    dc_r: f32,
    fixed: FixedRate,
    board: B,
}

impl<B: Board> ChipInst<B> {
    pub(crate) fn new(rate: u32, values: &[f32]) -> Self {
        let mut applied = [0i32; MAX_PARAMS];
        for (k, p) in B::params().iter().enumerate().take(MAX_PARAMS) {
            applied[k] = p.stored(values.get(k).copied().unwrap_or(p.default));
        }
        let chip = Chip::new(B::KIND);
        let chip_rate = chip.as_ref().map_or(rate, Chip::rate);
        ChipInst {
            chip,
            applied,
            alloc: Voices::new(),
            notes: [60.0; MAX_VOICES],
            dirty: true,
            retune: false,
            bend: 0.0,
            quiet: 0,
            asleep: true,
            dc: [0.0; 4],
            // A corner at 5 Hz.
            dc_r: 1.0 - std::f32::consts::TAU * 5.0 / chip_rate.max(1) as f32,
            fixed: FixedRate::new(chip_rate, rate, BLOCK),
            board: B::default(),
        }
    }

    fn limit(&self) -> usize {
        (self.applied[B::POLY].max(1) as usize)
            .min(B::VOICES)
            .min(MAX_VOICES)
    }

    fn pitch(&self, ch: usize) -> f32 {
        self.notes[ch] + self.applied[B::TUNE] as f32 + self.bend
    }

    /// Writes what changed since the last block: the patch, then the pitches.
    fn sync(&mut self) {
        let dirty = std::mem::take(&mut self.dirty);
        let retune = std::mem::take(&mut self.retune);
        if !dirty && !retune {
            return;
        }
        let pitches: [f32; MAX_VOICES] = std::array::from_fn(|ch| self.pitch(ch));
        let Some(chip) = self.chip.as_mut() else {
            return;
        };
        if dirty {
            self.board.program(chip, &self.applied);
        }
        for (ch, pitch) in pitches.iter().enumerate().take(B::VOICES) {
            if self.alloc.is_busy(ch) {
                let held = self.alloc.is_held(ch);
                self.board.retune(chip, ch, *pitch, held, &self.applied);
            }
        }
    }
}

impl<B: Board> NativeInst for ChipInst<B> {
    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = B::params().get(index) else {
            return;
        };
        let v = p.stored(value);
        if index < MAX_PARAMS && self.applied[index] != v {
            self.applied[index] = v;
            if index == B::TUNE {
                self.retune = true;
            } else if index != B::POLY && index != B::LEVEL {
                self.dirty = true;
            }
        }
    }

    fn param(&self, index: usize) -> f32 {
        if index < B::params().len() {
            self.applied.get(index).map_or(0.0, |v| *v as f32)
        } else {
            0.0
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.sync();
        let ch = self.alloc.start(note, self.limit());
        self.notes[ch] = f32::from(note);
        let pitch = self.pitch(ch);
        if let Some(chip) = self.chip.as_mut() {
            // A voice taken while it sounds is let go first: the chip starts a note on the
            // key's way down.
            self.board.key_off(chip, ch, pitch, &self.applied);
            let velocity = f32::from(velocity) / 127.0;
            self.board.key_on(chip, ch, pitch, velocity, &self.applied);
        }
        self.asleep = false;
        self.quiet = 0;
    }

    fn note_off(&mut self, note: u8) {
        if let Some(ch) = self.alloc.stop(note) {
            let pitch = self.pitch(ch);
            if let Some(chip) = self.chip.as_mut() {
                self.board.key_off(chip, ch, pitch, &self.applied);
            }
        }
    }

    fn all_notes_off(&mut self) {
        let pitches: [f32; MAX_VOICES] = std::array::from_fn(|ch| self.pitch(ch));
        let (chip, applied, board) = (&mut self.chip, &self.applied, &mut self.board);
        self.alloc.stop_all(|ch| {
            if let Some(chip) = chip.as_mut() {
                board.key_off(chip, ch, pitches[ch], applied);
            }
        });
    }

    fn all_sound_off(&mut self) {
        self.alloc.clear();
        if let Some(chip) = self.chip.as_mut() {
            chip.reset();
        }
        self.dirty = true;
        self.asleep = true;
        self.dc = [0.0; 4];
        self.fixed.reset();
    }

    fn bend(&mut self, semitones: f32) {
        if self.bend != semitones {
            self.bend = semitones;
            self.retune = true;
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.sync();
        let ChipInst {
            chip,
            applied,
            alloc,
            quiet,
            asleep,
            dc,
            dc_r,
            fixed,
            board,
            ..
        } = self;
        let Some(chip) = chip.as_mut() else {
            left.fill(0.0);
            right.fill(0.0);
            return;
        };
        let gain = 10f32.powf(applied[B::LEVEL] as f32 / 20.0);
        fixed.pull(left, right, |l, r| {
            if *asleep {
                l.fill(0.0);
                r.fill(0.0);
                return;
            }
            board.tick(chip, l.len(), &applied[..]);
            chip.render(l, r);
            let mut peak = 0.0f32;
            for (side, buf) in [&mut *l, &mut *r].into_iter().enumerate() {
                let (mut x1, mut y1) = (dc[side * 2], dc[side * 2 + 1]);
                for v in buf.iter_mut() {
                    let y = *v - x1 + *dc_r * y1;
                    (x1, y1) = (*v, y);
                    peak = peak.max(y.abs());
                    *v = y * gain;
                }
                (dc[side * 2], dc[side * 2 + 1]) = (x1, y1);
            }
            let held = (0..B::VOICES).any(|ch| alloc.is_held(ch));
            *quiet = if peak < QUIET && !held { *quiet + 1 } else { 0 };
            if *quiet >= QUIET_BLOCKS {
                *asleep = true;
                // Every voice has rung out.
                alloc.clear();
            }
        });
    }

    fn latency_seconds(&self) -> f64 {
        self.fixed.lag_seconds()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequencies_become_block_and_f_number() {
        assert!((hz(69.0) - 440.0).abs() < 1e-3 && (hz(57.0) - 220.0).abs() < 1e-3);
        // 440 Hz on a chip at 49716 Hz with 10-bit F-numbers over 2^20: block 4, 580.
        let (block, f) = block_fnum(440.0, 49716.0, 20, 10, 8);
        assert_eq!((block, f), (4, 580));
        let back = f64::from(f) * 49716.0 / f64::from(1u32 << (20 - block));
        assert!((back / 440.0 - 1.0).abs() < 0.002, "{back}");
        // Too high for the top block: the highest the chip plays.
        assert_eq!(block_fnum(1.0e6, 49716.0, 20, 10, 8), (7, 1023));
        assert_eq!(block_fnum(0.0, 49716.0, 20, 10, 8), (0, 0));
    }
}
