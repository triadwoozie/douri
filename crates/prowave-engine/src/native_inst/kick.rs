// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The sound is ported from LMMS's Kicker
// (plugins/Kicker/KickerOsc.h and Kicker.cpp, and the distortion of include/DspEffectLibrary.h;
// Copyright (c) 2006-2014 Tobias Doerffel, Copyright (c) 2014 grejppi; GPL-2.0-or-later, taken
// here under GPL-3.0-or-later; https://github.com/LMMS/lmms at commit f085f1e). Pro Wave's
// voices, parameter table and tests are its own.

//! Kick: a drum made of a sine that sweeps from one frequency down to another, with noise,
//! a click and a distortion whose amount can change over the hit.
//!
//! Logic Pro equivalent: Drum Synth's kicks, and Ultrabeat's oscillator 1 with a pitch
//! envelope.
//!
//! Each hit runs for Length: the level falls as 1 - (t / Length)^Envelope and the frequency
//! from Start to End as (t / Length)^Slope, where t is the time since the key. With Start from
//! Note or End to Note that end of the sweep is the key's pitch. Click is the phase the sine
//! starts at. A key let go early fades the hit out in 12 ms, as in the original.

use super::voices::Voices;
use super::{stored, NativeInst};
use crate::native_fx::NativeParam;

pub const ID: &str = "prowave:kick";
pub const NAME: &str = "Kick";

pub const START: usize = 0;
pub const END: usize = 1;
pub const LENGTH: usize = 2;
pub const DIST_START: usize = 3;
pub const DIST_END: usize = 4;
pub const GAIN: usize = 5;
pub const ENVELOPE: usize = 6;
pub const NOISE: usize = 7;
pub const CLICK: usize = 8;
pub const SLOPE: usize = 9;
pub const START_NOTE: usize = 10;
pub const END_NOTE: usize = 11;
pub const LEVEL: usize = 12;
pub const COUNT: usize = 13;

/// The original's ranges and defaults; the distortions are in tenths, Gain, Noise and Click
/// in percent, the two slopes in thousandths.
pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::knob("Start Frequency", 5, 1000, 150, "Hz"),
    NativeParam::knob("End Frequency", 5, 1000, 40, "Hz"),
    NativeParam::knob("Length", 5, 5000, 440, "ms"),
    NativeParam::knob("Start Distortion", 0, 1000, 8, ""),
    NativeParam::knob("End Distortion", 0, 1000, 8, ""),
    NativeParam::knob("Gain", 10, 500, 100, "%"),
    NativeParam::knob("Envelope Slope", 10, 1000, 163, ""),
    NativeParam::knob("Noise", 0, 100, 0, "%"),
    NativeParam::knob("Click", 0, 100, 40, "%"),
    NativeParam::knob("Frequency Slope", 1, 1000, 60, ""),
    NativeParam::choice("Start from Note", 0, 1, &["Off", "On"]),
    NativeParam::choice("End to Note", 0, 0, &["Off", "On"]),
    NativeParam::knob("Level", -48, 12, -6, "dB"),
];

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        START | END => format!("{v}Hz"),
        LENGTH => format!("{v}ms"),
        DIST_START | DIST_END => format!("{:.1}", v as f32 / 10.0),
        GAIN | NOISE | CLICK => format!("{v}%"),
        ENVELOPE | SLOPE => format!("{:.3}", v as f32 / 1000.0),
        START_NOTE | END_NOTE => if v != 0 { "On" } else { "Off" }.to_string(),
        LEVEL => format!("{v}dB"),
        _ => String::new(),
    }
}

/// Hits that can sound at once.
const MAX_VOICES: usize = 4;
/// Seconds a hit takes to fade when its key goes up before its end (the original's 512 frames
/// at 44.1 kHz).
const RELEASE_SECONDS: f32 = 512.0 / 44100.0;

/// One hit: `KickerOsc`, with the values it was struck with.
#[derive(Clone, Copy, Default)]
struct Hit {
    phase: f32,
    start_hz: f32,
    end_hz: f32,
    noise: f32,
    slope: f32,
    env: f32,
    dist_start: f32,
    dist_end: f32,
    dist_gain: f32,
    /// Frames the hit lasts.
    length: f32,
    counter: f32,
    hz: f64,
    velocity: f32,
    /// The fade after an early key-up: 1 while held, falling to 0.
    release: f32,
    released: bool,
}

/// LMMS's `DspEffectLibrary::Distortion`.
#[inline]
fn distort(x: f32, threshold: f32, gain: f32) -> f32 {
    let a = x.abs();
    gain * (x * (a + threshold) / (x * x + (threshold - 1.0) * a + 1.0))
}

/// The instrument.
pub struct Kick {
    rate: f32,
    applied: [i32; COUNT],
    hits: [Hit; MAX_VOICES],
    alloc: Voices<MAX_VOICES>,
    seed: u32,
}

impl Kick {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        Kick {
            rate: rate as f32,
            applied: stored(&PARAMS, values),
            hits: [Hit::default(); MAX_VOICES],
            alloc: Voices::new(),
            seed: 0x2545_f491,
        }
    }
}

impl NativeInst for Kick {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = PARAMS.get(index) {
            self.applied[index] = p.stored(value);
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        let p = &self.applied;
        let note_hz = 440.0 * ((f32::from(note) - 69.0) / 12.0).exp2();
        let noise = p[NOISE] as f32 / 100.0;
        let start_hz = if p[START_NOTE] != 0 {
            note_hz
        } else {
            p[START] as f32
        };
        let i = self.alloc.start(note, MAX_VOICES);
        self.hits[i] = Hit {
            phase: p[CLICK] as f32 / 100.0 * 0.25,
            start_hz,
            end_hz: if p[END_NOTE] != 0 {
                note_hz
            } else {
                p[END] as f32
            },
            noise: noise * noise,
            slope: p[SLOPE] as f32 / 1000.0,
            env: p[ENVELOPE] as f32 / 1000.0,
            dist_start: p[DIST_START] as f32 / 10.0,
            dist_end: p[DIST_END] as f32 / 10.0,
            dist_gain: p[GAIN] as f32 / 100.0,
            length: (p[LENGTH] as f32 * 0.001 * self.rate).max(1.0),
            counter: 0.0,
            hz: f64::from(start_hz),
            velocity: f32::from(velocity) / 127.0,
            release: 1.0,
            released: false,
        };
    }

    fn note_off(&mut self, note: u8) {
        if let Some(i) = self.alloc.stop(note) {
            self.hits[i].released = true;
        }
    }

    fn all_notes_off(&mut self) {
        let hits = &mut self.hits;
        self.alloc.stop_all(|i| hits[i].released = true);
    }

    fn all_sound_off(&mut self) {
        self.alloc.clear();
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        let gain = 10f32.powf(self.applied[LEVEL] as f32 / 20.0);
        let release_step = 1.0 / (RELEASE_SECONDS * self.rate).max(1.0);
        for (i, h) in self.hits.iter_mut().enumerate() {
            if !self.alloc.is_busy(i) {
                continue;
            }
            let level = gain * h.velocity;
            for out in left.iter_mut() {
                let t = if h.counter < h.length {
                    h.counter / h.length
                } else {
                    1.0
                };
                let g = 1.0 - t.powf(h.env);
                // The original's noise is its fast random number in -1 to 1.
                self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let rnd = (self.seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
                let s = (h.phase * std::f32::consts::TAU).sin() * (1.0 - h.noise)
                    + rnd * g * g * h.noise;
                let threshold = h.dist_start + (h.dist_end - h.dist_start) * t;
                *out += distort(s * g, threshold, h.dist_gain) * h.release * level;
                h.phase += (h.hz / f64::from(self.rate)) as f32;
                h.phase -= h.phase.floor();
                let change = if h.counter < h.length {
                    (h.start_hz - h.end_hz) * (1.0 - t.powf(h.slope))
                } else {
                    0.0
                };
                h.hz = f64::from(h.end_hz + change);
                h.counter += 1.0;
                if h.released {
                    h.release = (h.release - release_step).max(0.0);
                }
            }
        }
        // A hit that is over frees its voice (its key may still be down).
        for (i, h) in self.hits.iter().enumerate() {
            if self.alloc.is_busy(i) && (h.counter >= h.length || h.release <= 0.0) {
                self.alloc.release_voice(i);
            }
        }
        right.copy_from_slice(left);
    }
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, play, sounds_and_falls_silent};
    use super::*;

    #[test]
    fn kick_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 1.0);
        sounds_and_falls_silent(ID, 44100, &[], 1.0);
    }

    /// The hit lasts Length and no longer, with the key still down, and its frequency falls
    /// from the start to the end.
    #[test]
    fn the_hit_sweeps_down_and_ends_at_its_length() {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[START_NOTE] = 0.0;
        values[START] = 400.0;
        values[END] = 50.0;
        values[LENGTH] = 500.0;
        values[CLICK] = 0.0;
        // A slow, straight sweep.
        values[SLOPE] = 1000.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, 60, 127)]);
        assert!(peak(&l[..2400]) > 0.1);
        assert_eq!(peak(&l[24100..]), 0.0, "silent after 500 ms");
        let crossings = |a: usize, b: usize| {
            l[a..b]
                .windows(2)
                .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
                .count()
        };
        // 400 Hz falling in a straight line to 50 Hz: 365 Hz on average over the first 100 ms
        // (36 cycles), 155 Hz over the fourth (15).
        let (early, late) = (crossings(0, 4800), crossings(14400, 19200));
        assert!(
            (33..=39).contains(&early) && (13..=18).contains(&late),
            "{early} {late}"
        );
    }

    /// Start from Note makes the key the top of the sweep; a key let go early fades the hit.
    #[test]
    fn the_key_sets_the_start_and_cuts_the_hit_short() {
        let crossings = |note: u8| {
            let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 2400, 256, &[(0, 0x90, note, 127)]);
            l.windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count()
        };
        assert!(
            crossings(84) > crossings(48) * 2,
            "{} {}",
            crossings(84),
            crossings(48)
        );
        let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
            panic!("does not open")
        };
        let (l, _) = play(
            &mut inst,
            9600,
            256,
            &[(0, 0x90, 48, 127), (960, 0x80, 48, 0)],
        );
        assert!(peak(&l[..960]) > 0.1);
        assert_eq!(peak(&l[1700..]), 0.0, "faded 12 ms after the key went up");
    }

    #[test]
    fn the_distortion_is_the_originals() {
        // At threshold 1 the curve is x (|x| + 1) / (x^2 + 1).
        assert!((distort(0.5, 1.0, 1.0) - 0.6).abs() < 1e-6);
        assert!((distort(-0.5, 1.0, 2.0) + 1.2).abs() < 1e-6);
        assert_eq!(distort(0.0, 0.8, 1.0), 0.0);
    }
}
