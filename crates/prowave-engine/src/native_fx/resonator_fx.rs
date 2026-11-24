// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The resonator is the vendored crate crates/mi-dsp
// (MIT, Copyright 2015 Emilie Gillet, https://github.com/pichenettes/eurorack, rings/dsp at
// commit f4cfa76), its C++ unchanged; see its NOTICE.

//! Pro Wave Resonator FX: the track's sound excites a tuned modal or string resonator.
//!
//! Logic Pro equivalent: none directly; nearest is Sculpture played by a side chain.
//!
//! The same resonator as the instrument ([`crate::native_inst::resonator`]), with the input
//! (both channels summed) in place of its exciter: the resonator rings at Note, and each onset
//! in the input strums it (with 2 or 4 voices, the next voice). It runs at 48 kHz in blocks of
//! 24 frames behind a converter ([`FixedRateFx`]), whose fixed delay is reported; the dry
//! signal in the Mix is delayed by the same amount.

use super::{copy_in, stored_values, NativeFx, NativeParam};
use crate::native_inst::rate::FixedRateFx;
use crate::native_inst::resonator::{polyphony, MODELS, VOICE_COUNTS};
use crate::plugin_track::BlockProcessor;
use mi_dsp::{Resonator as Dsp, ResonatorBlock, RESONATOR_BLOCK, RESONATOR_RATE};

pub const ID: &str = "prowave:resonator-fx";
pub const NAME: &str = "Pro Wave Resonator FX";

pub const MODEL: usize = 0;
pub const VOICES: usize = 1;
pub const NOTE: usize = 2;
pub const STRUCTURE: usize = 3;
pub const BRIGHTNESS: usize = 4;
pub const DAMPING: usize = 5;
pub const POSITION: usize = 6;
pub const CHORD: usize = 7;
pub const MIX: usize = 8;
pub const COUNT: usize = 9;

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::choice("Model", 0, 0, &MODELS),
    NativeParam::choice("Voices", 0, 0, &VOICE_COUNTS),
    NativeParam::knob("Note", 24, 96, 48, ""),
    NativeParam::knob("Structure", 0, 100, 25, "%"),
    NativeParam::knob("Brightness", 0, 100, 50, "%"),
    NativeParam::knob("Damping", 0, 100, 40, "%"),
    NativeParam::knob("Position", 0, 100, 30, "%"),
    NativeParam::knob("Chord", 1, 11, 1, ""),
    NativeParam::knob("Mix", 0, 100, 50, "%"),
];

/// A MIDI note's name ("C3" is 60, as in Logic).
fn note_name(note: i32) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!(
        "{}{}",
        NAMES[note.rem_euclid(12) as usize],
        note.div_euclid(12) - 2
    )
}

pub fn value_text(k: usize, v: i32) -> String {
    let pick = |names: &[&str]| {
        names
            .get(v.max(0) as usize)
            .map_or_else(String::new, |s| (*s).to_string())
    };
    match k {
        MODEL => pick(&MODELS),
        VOICES => pick(&VOICE_COUNTS),
        NOTE => note_name(v),
        CHORD => v.to_string(),
        _ if k < COUNT => format!("{v}%"),
        _ => String::new(),
    }
}

/// The effect.
pub struct ResonatorFx {
    rate: u32,
    applied: [i32; COUNT],
    /// `None` when memory ran out: the effect passes its input on.
    dsp: Option<Dsp>,
    fixed: FixedRateFx,
}

impl ResonatorFx {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        ResonatorFx {
            rate,
            applied: stored_values(&PARAMS, values),
            dsp: Dsp::new(),
            fixed: FixedRateFx::new(RESONATOR_RATE, rate, RESONATOR_BLOCK),
        }
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32]) {
        let p = self.applied;
        let Some(dsp) = self.dsp.as_mut() else { return };
        let unit = |k: usize| p[k] as f32 / 100.0;
        let block = ResonatorBlock {
            model: p[MODEL],
            polyphony: polyphony(p[VOICES]),
            structure: unit(STRUCTURE),
            brightness: unit(BRIGHTNESS),
            damping: unit(DAMPING),
            position: unit(POSITION),
            note: p[NOTE] as f32,
            fm: 0.0,
            chord: p[CHORD] - 1,
            strum: 0,
            external: 1,
        };
        let wet = unit(MIX);
        let mut mono = [0.0f32; RESONATOR_BLOCK];
        self.fixed.process(
            left,
            right,
            |il, ir, ol, or| {
                for ((m, a), b) in mono.iter_mut().zip(il).zip(ir) {
                    *m = if a.is_finite() && b.is_finite() {
                        (a + b) * 0.5
                    } else {
                        0.0
                    };
                }
                dsp.process(&block, Some(&mono), ol, or);
            },
            |dry, out| dry * (1.0 - wet) + out * wet,
        );
    }
}

impl BlockProcessor for ResonatorFx {
    fn midi(&mut self, _status: u8, _data1: u8, _data2: u8, _offset: u32) {}

    fn process(&mut self, input: (&[f32], &[f32]), out: (&mut [f32], &mut [f32])) {
        let n = copy_in(input, (&mut *out.0, &mut *out.1));
        self.run(&mut out.0[..n], &mut out.1[..n]);
        for v in out.0[..n].iter_mut().chain(out.1[..n].iter_mut()) {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
    }

    fn latency_seconds(&self) -> f64 {
        if self.dsp.is_some() {
            self.fixed.latency_frames() as f64 / f64::from(self.rate)
        } else {
            0.0
        }
    }
}

impl NativeFx for ResonatorFx {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = PARAMS.get(index) {
            self.applied[index] = p.stored(value);
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_signals::{noise, peak, run_blocks};
    use super::*;

    #[test]
    fn resonator_fx_does_not_allocate() {
        let mut fx = ResonatorFx::new(44100, &[]);
        let (l, r) = (noise(44100, 0.5, 3), noise(44100, 0.5, 4));
        let (a, b) = run_blocks(&mut fx, &l, &r, 256);
        assert!(a.iter().chain(&b).all(|v| v.is_finite()));
        assert!(peak(&a) > 0.01);
        assert_no_alloc::assert_no_alloc(|| {
            for (k, p) in PARAMS.iter().enumerate() {
                fx.set_param(k, p.max);
            }
        });
        let (a, _) = run_blocks(&mut fx, &l, &r, 100);
        assert!(a.iter().all(|v| v.is_finite()));
        assert_eq!(fx.fixed.underruns(), 0);
    }

    /// An impulse leaves a tail that rings at the Note and dies away.
    #[test]
    fn an_impulse_rings_at_the_note_and_decays() {
        // The plain string, one voice, A2 (110 Hz), all wet.
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[MODEL] = 2.0;
        values[NOTE] = 45.0;
        values[MIX] = 100.0;
        let mut fx = ResonatorFx::new(48000, &values);
        let n = 48000 * 3;
        let mut x = vec![0.0f32; n];
        for v in &mut x[1000..1040] {
            *v = 0.9;
        }
        let (l, r) = run_blocks(&mut fx, &x, &x, 256);
        let sum: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a + b).collect();
        let early = peak(&sum[2000..12000]);
        let late = peak(&sum[n - 12000..]);
        assert!(early > 0.01, "{early}");
        assert!(late < early * 0.25, "early {early}, late {late}");
        // Its partials are the harmonics of the Note.
        let ratio = crate::native_inst::test_notes::tuned_to(&sum[4000..28000], 110.0, 48000.0);
        assert!(ratio > 5.0, "{ratio}");
        assert_eq!(value_text(NOTE, 45), "A1");
        assert_eq!(value_text(NOTE, 60), "C3");
    }

    /// All dry, the output is the input, late by the reported delay.
    #[test]
    fn the_dry_signal_is_delayed_by_the_reported_latency() {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[MIX] = 0.0;
        let mut fx = ResonatorFx::new(44100, &values);
        let lat = (fx.latency_seconds() * 44100.0).round() as usize;
        assert!(lat > 0);
        let x = noise(8000, 0.5, 9);
        let (l, _) = run_blocks(&mut fx, &x, &x, 512);
        assert_eq!(&l[lat..], &x[..8000 - lat]);
    }
}
