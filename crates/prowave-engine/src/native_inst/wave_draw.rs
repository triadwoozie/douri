// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The oscillator is ported from LMMS's BitInvader
// (plugins/BitInvader/BitInvader.cpp and .h; Copyright (c) 2006-2008 Andreas Brandmaier;
// GPL-2.0-or-later, taken here under GPL-3.0-or-later; https://github.com/LMMS/lmms at commit
// f085f1e). Pro Wave's voices, envelope, parameter table and tests are its own.

//! Wave Draw: an oscillator whose single cycle is drawn by hand, 4 to 200 points long.
//!
//! Logic Pro equivalent: none; Retro Synth's Table engine plays wavetables, but does not let
//! one be drawn.
//!
//! The wave is 200 points, of which the first Length are the cycle; a note steps through them
//! at its frequency, taking the point it is on or, with Interpolation, a straight line between
//! two. Normalize scales the cycle so its largest point is full scale. Shorter cycles with no
//! interpolation give the sound of early sample chips. The points are parameters 9 to 208, so
//! a drawn wave is saved, automated and set as every other setting is. The original takes its
//! envelope from LMMS's instrument track; here each voice has Pro Wave's ([`Adsr`]).

use super::envelope::{Adsr, AdsrRates, AdsrTimes};
use super::voices::Voices;
use super::NativeInst;
use crate::native_fx::NativeParam;
use std::sync::OnceLock;

pub const ID: &str = "prowave:wave-draw";
pub const NAME: &str = "Wave Draw";

pub const LENGTH: usize = 0;
pub const INTERPOLATION: usize = 1;
pub const NORMALIZE: usize = 2;
pub const ATTACK: usize = 3;
pub const DECAY: usize = 4;
pub const SUSTAIN: usize = 5;
pub const RELEASE: usize = 6;
pub const VOICES: usize = 7;
pub const LEVEL: usize = 8;
/// The wave's first point; each is -100 to 100.
pub const WAVE: usize = 9;
/// Points in the wave (the original's table).
pub const POINTS: usize = 200;
pub const COUNT: usize = WAVE + POINTS;

/// Most voices at once.
pub const MAX_VOICES: usize = 8;

/// A drawn shape for the whole table: point `i` of `POINTS`, -100 to 100.
pub fn shape(name: &str, i: usize) -> i32 {
    let t = i as f32 / POINTS as f32;
    let y = match name {
        "Triangle" => {
            if t < 0.25 {
                4.0 * t
            } else if t < 0.75 {
                2.0 - 4.0 * t
            } else {
                4.0 * t - 4.0
            }
        }
        "Saw" => 2.0 * t - 1.0,
        "Square" => {
            if t < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        // The original's starting wave.
        _ => (t * std::f32::consts::TAU).sin(),
    };
    (y * 100.0).round() as i32
}

/// The shapes the window's keys draw.
pub static SHAPES: [&str; 4] = ["Sine", "Triangle", "Saw", "Square"];

pub fn params() -> &'static [NativeParam] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    static PARAMS: OnceLock<Vec<NativeParam>> = OnceLock::new();
    PARAMS.get_or_init(|| {
        let names = NAMES.get_or_init(|| (1..=POINTS).map(|i| format!("Point {i}")).collect());
        let mut out = vec![
            NativeParam::knob("Length", 4, POINTS as i32, POINTS as i32, ""),
            NativeParam::choice("Interpolation", 0, 0, &["Off", "On"]),
            NativeParam::choice("Normalize", 0, 0, &["Off", "On"]),
            NativeParam::knob("Attack", 0, 2000, 2, "ms"),
            NativeParam::knob("Decay", 0, 5000, 400, "ms"),
            NativeParam::knob("Sustain", 0, 100, 80, "%"),
            NativeParam::knob("Release", 0, 5000, 100, "ms"),
            NativeParam::knob("Voices", 1, MAX_VOICES as i32, MAX_VOICES as i32, ""),
            NativeParam::knob("Level", -48, 12, -6, "dB"),
        ];
        for (i, name) in names.iter().enumerate() {
            out.push(NativeParam::knob(
                name.as_str(),
                -100,
                100,
                shape("Sine", i),
                "",
            ));
        }
        out
    })
}

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        INTERPOLATION | NORMALIZE => if v != 0 { "On" } else { "Off" }.to_string(),
        ATTACK | DECAY | RELEASE => format!("{v}ms"),
        SUSTAIN => format!("{v}%"),
        LEVEL => format!("{v}dB"),
        _ if k < COUNT => v.to_string(),
        _ => String::new(),
    }
}

struct Voice {
    env: Adsr,
    hz: f32,
    velocity: f32,
    /// Where the voice is in the cycle, in points (`sample_realindex`).
    index: f32,
}

/// The instrument.
pub struct WaveDraw {
    rate: f32,
    applied: [i32; COUNT],
    /// The wave as samples: the points over 100, times the normalizing factor, clipped.
    table: [f32; POINTS],
    dirty: bool,
    voices: [Voice; MAX_VOICES],
    alloc: Voices<MAX_VOICES>,
    bend: f32,
}

impl WaveDraw {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let mut applied = [0i32; COUNT];
        for (k, p) in params().iter().enumerate() {
            applied[k] = p.stored(values.get(k).copied().unwrap_or(p.default));
        }
        WaveDraw {
            rate: rate.max(1) as f32,
            applied,
            table: [0.0; POINTS],
            dirty: true,
            voices: std::array::from_fn(|_| Voice {
                env: Adsr::new(),
                hz: 440.0,
                velocity: 0.0,
                index: 0.0,
            }),
            alloc: Voices::new(),
            bend: 0.0,
        }
    }

    /// The table from the points (`BitInvader::normalize` and `BSynth`'s constructor).
    fn rebuild(&mut self) {
        let length = (self.applied[LENGTH].clamp(4, POINTS as i32)) as usize;
        let points = &self.applied[WAVE..];
        let factor = if self.applied[NORMALIZE] != 0 {
            let max = points[..length]
                .iter()
                .fold(f32::EPSILON, |m, v| m.max(v.abs() as f32 / 100.0));
            1.0 / max
        } else {
            1.0
        };
        for (t, v) in self.table.iter_mut().zip(points) {
            *t = (*v as f32 / 100.0 * factor).clamp(-1.0, 1.0);
        }
    }
}

impl NativeInst for WaveDraw {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = params().get(index) {
            let v = p.stored(value);
            if self.applied[index] != v {
                self.applied[index] = v;
                self.dirty |= index >= WAVE || index == LENGTH || index == NORMALIZE;
            }
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        let limit = (self.applied[VOICES].max(1) as usize).min(MAX_VOICES);
        let i = self.alloc.start(note, limit);
        let v = &mut self.voices[i];
        v.hz = 440.0 * ((f32::from(note) - 69.0) / 12.0).exp2();
        v.velocity = f32::from(velocity) / 127.0;
        v.index = 0.0;
        v.env.gate_on();
    }

    fn note_off(&mut self, note: u8) {
        if let Some(i) = self.alloc.stop(note) {
            self.voices[i].env.gate_off();
        }
    }

    fn all_notes_off(&mut self) {
        let voices = &mut self.voices;
        self.alloc.stop_all(|i| voices[i].env.gate_off());
    }

    fn all_sound_off(&mut self) {
        self.alloc.clear();
        for v in &mut self.voices {
            v.env.kill();
        }
    }

    fn bend(&mut self, semitones: f32) {
        self.bend = semitones;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        if std::mem::take(&mut self.dirty) {
            self.rebuild();
        }
        left.fill(0.0);
        let p = &self.applied;
        let length = p[LENGTH].clamp(4, POINTS as i32) as usize;
        let interpolate = p[INTERPOLATION] != 0;
        let rates = AdsrRates::new(
            &AdsrTimes {
                attack_s: p[ATTACK] as f32 * 0.001,
                decay_s: p[DECAY] as f32 * 0.001,
                sustain: p[SUSTAIN] as f32 / 100.0,
                release_s: p[RELEASE] as f32 * 0.001,
            },
            self.rate,
        );
        let gain = 0.5 * 10f32.powf(p[LEVEL] as f32 / 20.0);
        let bend = (self.bend / 12.0).exp2();
        for (i, v) in self.voices.iter_mut().enumerate() {
            if !self.alloc.is_busy(i) {
                continue;
            }
            // `nextStringSample`: the cycle's points per sample at the note's frequency.
            let step = length as f32 / (self.rate / (v.hz * bend));
            for out in left.iter_mut() {
                while v.index >= length as f32 {
                    v.index -= length as f32;
                }
                let at = (v.index as usize).min(length - 1);
                let s = if interpolate {
                    let next = if at + 1 < length { at + 1 } else { 0 };
                    let f = v.index - at as f32;
                    self.table[at] + (self.table[next] - self.table[at]) * f
                } else {
                    self.table[at]
                };
                v.index += step;
                *out += s * rates.tick(&mut v.env) * v.velocity * gain;
            }
            if v.env.is_idle() {
                self.alloc.idle(i);
            }
        }
        right.copy_from_slice(left);
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_notes::{peak, pitch, play, sounds_and_falls_silent};
    use super::super::{open_native_instrument, BlockProcessor};
    use super::*;

    #[test]
    fn wave_draw_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 2.0);
        sounds_and_falls_silent(ID, 44100, &[], 2.0);
    }

    #[test]
    fn the_note_sets_the_pitch_at_any_length() {
        for length in [200.0f32, 32.0, 4.0] {
            for (note, want) in [(57u8, 220.0f64), (69, 440.0)] {
                // One cycle of a sine drawn over the length.
                let mut values: Vec<f32> = params().iter().map(|p| p.default).collect();
                values[LENGTH] = length;
                for i in 0..length as usize {
                    let t = i as f32 / length * std::f32::consts::TAU;
                    values[WAVE + i] = (t.sin() * 100.0).round();
                }
                let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                    panic!("does not open")
                };
                let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 100)]);
                assert!(peak(&l) > 0.01, "length {length}");
                let got = pitch(&l[24000..], 48000.0, 60.0, 1500.0);
                assert!(
                    (got / want - 1.0).abs() < 0.01,
                    "length {length}: {note} is {got} Hz"
                );
            }
        }
    }

    /// The points are the wave: a square drawn in is a square out, a quiet wave is made full
    /// scale by Normalize, and a point moved while a note sounds is heard.
    #[test]
    fn the_drawn_points_are_what_sounds() {
        let mut values: Vec<f32> = params().iter().map(|p| p.default).collect();
        values[SUSTAIN] = 100.0;
        values[ATTACK] = 0.0;
        for i in 0..POINTS {
            values[WAVE + i] = shape("Square", i) as f32 / 4.0;
        }
        let run = |values: &[f32]| {
            let Some(mut inst) = open_native_instrument(ID, 48000, values) else {
                panic!("does not open")
            };
            play(&mut inst, 9600, 256, &[(0, 0x90, 45, 127)]).0
        };
        let quiet = run(&values);
        // A square at a quarter of full scale: two levels only, 0.5 * 10^(-6/20) * 0.25.
        let level = 0.5 * 10f32.powf(-0.3) * 0.25;
        assert!(quiet[100..].iter().all(|v| (v.abs() - level).abs() < 1e-4));
        values[NORMALIZE] = 1.0;
        let full = run(&values);
        assert!((peak(&full) / peak(&quiet) - 4.0).abs() < 0.01);
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        let (a, _) = play(&mut inst, 4800, 256, &[(0, 0x90, 45, 127)]);
        for i in 0..POINTS {
            inst.set_parameter(WAVE + i, 0.0);
        }
        let (b, _) = play(&mut inst, 4800, 256, &[]);
        assert!(peak(&a) > 0.1 && peak(&b[512..]) == 0.0);
        assert_eq!(
            [0, 50, 100, 150].map(|i| shape("Sine", i)),
            [0, 100, 0, -100]
        );
        assert_eq!(
            [0, 50, 100, 150].map(|i| shape("Triangle", i)),
            [0, 100, 0, -100]
        );
        assert_eq!([0, 100, 199].map(|i| shape("Saw", i)), [-100, 0, 99]);
    }
}
