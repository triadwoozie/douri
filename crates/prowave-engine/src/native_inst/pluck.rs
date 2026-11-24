// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The voice is ported from Bespoke Synth's
// Karplus-Strong voice (Source/KarplusStrongVoice.cpp and .h, with the parts of ADSR.cpp,
// MathUtils.cpp, RollingBuffer.cpp, SingleOscillatorVoice.cpp and KarplusStrong.cpp it uses;
// Copyright (C) 2021 Ryan Challinor; GPL-3.0-or-later;
// https://github.com/BespokeSynth/BespokeSynth at commit 25b1541). Pro Wave's voice
// allocation, parameter table and tests are its own.

//! Pluck: plucked strings by the Karplus-Strong method: a short burst goes round a delay as
//! long as the note's period, losing a little and being filtered a little each time round.
//!
//! Logic Pro equivalent: Sculpture's string, in its simplest form.
//!
//! Each voice has a delay line, read one period back with linear interpolation, a one-pole
//! filter on what comes back (brighter for higher notes, by Pitch Tone), and a Feedback that
//! sets how long the string rings. The burst that excites it is a sine or saw at Exciter
//! Frequency, noise, or a mix that leans to noise for higher notes, shaped by a short attack
//! and decay. A key let go mutes the string over 400 ms, as in the original. The original's
//! two "input" sources (another module's audio as the exciter) are not here.

use super::voices::Voices;
use super::{stored, NativeInst};
use crate::native_fx::NativeParam;

pub const ID: &str = "prowave:pluck";
pub const NAME: &str = "Pluck";

pub const SOURCE: usize = 0;
pub const FILTER: usize = 1;
pub const FEEDBACK: usize = 2;
pub const EXCITER_FREQ: usize = 3;
pub const EXCITER_ATTACK: usize = 4;
pub const EXCITER_DECAY: usize = 5;
pub const PITCH_TONE: usize = 6;
pub const VEL_TO_VOLUME: usize = 7;
pub const VEL_TO_ENVELOPE: usize = 8;
pub const INVERT: usize = 9;
pub const VOICES: usize = 10;
pub const LEVEL: usize = 11;
pub const COUNT: usize = 12;

const SOURCE_SINE: i32 = 0;
const SOURCE_NOISE: i32 = 1;
const SOURCE_MIX: i32 = 2;

/// The original's ranges and defaults: Filter, Pitch Tone and the exciter's times in
/// hundredths (the times of a millisecond), Feedback in ten-thousandths.
pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::choice("Source", 0, SOURCE_MIX, &["Sine", "Noise", "Mix", "Saw"]),
    NativeParam::knob("Filter", 0, 500, 100, ""),
    NativeParam::knob("Feedback", 5000, 9999, 9800, ""),
    NativeParam::knob("Exciter Frequency", 10, 1000, 100, "Hz"),
    NativeParam::knob("Exciter Attack", 1, 4000, 100, "ms"),
    NativeParam::knob("Exciter Decay", 1, 4000, 300, "ms"),
    NativeParam::knob("Pitch Tone", -200, 200, 0, ""),
    NativeParam::knob("Velocity to Volume", 0, 100, 100, "%"),
    NativeParam::knob("Velocity to Envelope", -100, 100, 50, "%"),
    NativeParam::choice("Invert", 0, 0, &["Off", "On"]),
    NativeParam::knob("Voices", 1, 8, 8, ""),
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
        FILTER | PITCH_TONE => format!("{:.2}", v as f32 / 100.0),
        FEEDBACK => format!("{:.4}", v as f32 / 10000.0),
        EXCITER_FREQ => format!("{v}Hz"),
        EXCITER_ATTACK | EXCITER_DECAY => format!("{:.2}ms", v as f32 / 100.0),
        VEL_TO_VOLUME | VEL_TO_ENVELOPE => format!("{v}%"),
        VOICES => v.to_string(),
        LEVEL => format!("{v}dB"),
        _ => String::new(),
    }
}

/// Most voices at once.
pub const MAX_VOICES: usize = 8;
/// Seconds a let-go string takes to be muted.
const MUTE_SECONDS: f32 = 0.4;

/// Bespoke's `MathUtils::Curve`.
fn curve(t: f32, amount: f32) -> f32 {
    t.powf((-2.0 * amount).exp())
}

/// Bespoke's `SingleOscillatorVoice::GetADSRScale`.
fn adsr_scale(velocity: f32, vel_to_envelope: f32) -> f32 {
    if vel_to_envelope >= 0.0 {
        let from = (1.0 - vel_to_envelope).clamp(0.0, 1.0);
        from + (1.0 - from) * velocity
    } else {
        (1.0 + vel_to_envelope * velocity).clamp(0.001, 1.0)
    }
}

struct Voice {
    /// The string: one second of what went round.
    buffer: Vec<f32>,
    /// Where the next sample is written.
    at: usize,
    filtered: f32,
    osc_phase: f32,
    /// Milliseconds since the key.
    age_ms: f32,
    attack_ms: f32,
    decay_ms: f32,
    volume: f32,
    pitch: f32,
    /// 1, falling to 0 once the key is up.
    mute: f32,
    muting: bool,
}

impl Voice {
    /// The exciter's envelope (Bespoke's two-stage ADSR: a straight rise, a curved fall).
    fn envelope(&self) -> f32 {
        if self.age_ms < self.attack_ms {
            self.volume * (self.age_ms / self.attack_ms)
        } else {
            let t = ((self.age_ms - self.attack_ms) / self.decay_ms).clamp(0.0, 1.0);
            self.volume * (1.0 - curve(t, 0.5))
        }
    }
}

/// The instrument.
pub struct Pluck {
    rate: f32,
    applied: [i32; COUNT],
    voices: Vec<Voice>,
    alloc: Voices<MAX_VOICES>,
    bend: f32,
    seed: u32,
}

impl Pluck {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let rate = rate.max(1);
        Pluck {
            rate: rate as f32,
            applied: stored(&PARAMS, values),
            voices: (0..MAX_VOICES)
                .map(|_| Voice {
                    buffer: vec![0.0; rate as usize],
                    at: 0,
                    filtered: 0.0,
                    osc_phase: 0.0,
                    age_ms: 0.0,
                    attack_ms: 1.0,
                    decay_ms: 3.0,
                    volume: 0.0,
                    pitch: 60.0,
                    mute: 1.0,
                    muting: false,
                })
                .collect(),
            alloc: Voices::new(),
            bend: 0.0,
            seed: 0x51ed_270b,
        }
    }
}

impl NativeInst for Pluck {
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
        let limit = (p[VOICES].max(1) as usize).min(MAX_VOICES);
        let i = self.alloc.start(note, limit);
        let Some(v) = self.voices.get_mut(i) else {
            return;
        };
        let target = f32::from(velocity) / 127.0;
        let to_volume = p[VEL_TO_VOLUME] as f32 / 100.0;
        // `Start`: the velocity's part in the level and in the attack's length.
        v.volume = (1.0 - to_volume) + to_volume * target * target;
        let scale = adsr_scale(target, -(p[VEL_TO_ENVELOPE] as f32) / 100.0);
        v.attack_ms = (p[EXCITER_ATTACK] as f32 / 100.0 * scale).max(1e-4);
        v.decay_ms = (p[EXCITER_DECAY] as f32 / 100.0).max(1e-4);
        v.osc_phase = std::f32::consts::FRAC_PI_2;
        v.age_ms = 0.0;
        v.pitch = f32::from(note);
        v.mute = 1.0;
        v.muting = false;
        // A voice taken from a note that had ended starts from a silent string
        // (`ClearVoice`); one still ringing keeps what it holds, as the original's does.
    }

    fn note_off(&mut self, note: u8) {
        if let Some(v) = self.alloc.stop(note).and_then(|i| self.voices.get_mut(i)) {
            v.muting = true;
        }
    }

    fn all_notes_off(&mut self) {
        let voices = &mut self.voices;
        self.alloc.stop_all(|i| voices[i].muting = true);
    }

    fn all_sound_off(&mut self) {
        self.alloc.clear();
        for v in &mut self.voices {
            v.buffer.fill(0.0);
            v.filtered = 0.0;
        }
    }

    fn bend(&mut self, semitones: f32) {
        self.bend = semitones;
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        let p = self.applied;
        let rate = self.rate;
        let invert = p[INVERT] != 0;
        let source = p[SOURCE];
        let filter = p[FILTER] as f32 / 100.0;
        let tone = (p[PITCH_TONE] as f32 / 100.0).exp2();
        let feedback_gain = (p[FEEDBACK] as f32 / 10000.0).sqrt();
        let phase_inc = std::f32::consts::TAU * p[EXCITER_FREQ] as f32 / rate;
        let gain = 0.5 * 10f32.powf(p[LEVEL] as f32 / 20.0);
        let ms = 1000.0 / rate;
        let mute_step = 1.0 / (MUTE_SECONDS * rate);
        for (i, v) in self.voices.iter_mut().enumerate() {
            if !self.alloc.is_busy(i) {
                continue;
            }
            // Inverted, the string sounds an octave down: the original plays it an octave up.
            let pitch = v.pitch + self.bend + if invert { 12.0 } else { 0.0 };
            let hz = 440.0 * ((pitch - 69.0) / 12.0).exp2();
            let filter_rate = filter * (hz / 300.0).powf(tone);
            let filter_lerp = (-filter_rate).exp2().clamp(0.0, 1.0);
            let blend = ((pitch - 40.0) / 60.0).clamp(0.0, 1.0).powi(2);
            let size = v.buffer.len();
            let samples_ago = rate / hz;
            for out in left.iter_mut() {
                v.osc_phase += phase_inc;
                if v.osc_phase > std::f32::consts::TAU {
                    v.osc_phase -= std::f32::consts::TAU;
                }
                let osc = if source == 3 {
                    v.osc_phase / std::f32::consts::PI - 1.0
                } else {
                    v.osc_phase.sin()
                };
                self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let noise = (self.seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
                let sample = match source {
                    SOURCE_SINE | 3 => osc,
                    SOURCE_NOISE => noise,
                    _ => noise * blend + osc * (1.0 - blend),
                } * v.envelope();
                // The string one period back, between two samples.
                let mut back = 0.0;
                if samples_ago < size as f32 {
                    let whole = samples_ago as usize;
                    let read = |ago: usize| {
                        if ago < size {
                            v.buffer[(size + v.at - ago) % size]
                        } else {
                            0.0
                        }
                    };
                    let a = samples_ago - whole as f32;
                    back = (1.0 - a) * read(whole) + a * read(whole + 1);
                }
                v.filtered = back + (v.filtered - back) * filter_lerp;
                let mut feedback = v.filtered * feedback_gain * v.mute;
                if invert {
                    feedback = -feedback;
                }
                let y = sample + feedback;
                let y = if y.is_finite() { y } else { 0.0 };
                v.buffer[v.at] = y;
                v.at = (v.at + 1) % size;
                *out += y * gain;
                v.age_ms += ms;
                if v.muting {
                    v.mute = (v.mute - mute_step).max(0.0);
                }
            }
            if v.muting && v.mute <= 0.0 {
                // Muted: the string is cleared for its next note (`ClearVoice`).
                v.buffer.fill(0.0);
                v.filtered = 0.0;
                self.alloc.idle(i);
            }
        }
        right.copy_from_slice(left);
    }
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, play, sounds_and_falls_silent, tuned_to};
    use super::*;

    #[test]
    fn pluck_does_not_allocate() {
        sounds_and_falls_silent(ID, 48000, &[], 1.0);
        sounds_and_falls_silent(ID, 44100, &[], 1.0);
    }

    #[test]
    fn the_helpers_are_the_originals() {
        assert_eq!(curve(0.25, 0.0), 0.25);
        assert!((curve(0.5, 0.5) - 0.5f32.powf((-1.0f32).exp())).abs() < 1e-6);
        assert_eq!(adsr_scale(0.5, 0.0), 1.0);
        assert_eq!(adsr_scale(1.0, 0.5), 1.0);
        assert_eq!(adsr_scale(0.0, 0.5), 0.5);
        assert_eq!(adsr_scale(1.0, -0.5), 0.5);
        assert_eq!(adsr_scale(1.0, -1.0), 0.001);
    }

    /// The string rings at the note for every source, and an inverted string still does.
    #[test]
    fn the_note_sets_the_pitch() {
        for source in 0..4 {
            for invert in [0.0f32, 1.0] {
                for (note, hz) in [(45u8, 110.0f64), (57, 220.0), (69, 440.0)] {
                    let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
                    values[SOURCE] = source as f32;
                    values[INVERT] = invert;
                    let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                        panic!("does not open")
                    };
                    let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 100)]);
                    assert!(peak(&l) > 0.01, "source {source}: {}", peak(&l));
                    // An inverted string has only the odd harmonics of its halved pitch.
                    let ratio = tuned_to(&l[4800..28800], hz, 48000.0);
                    assert!(
                        ratio > 4.0,
                        "source {source}, invert {invert}, {note}: {ratio}"
                    );
                }
            }
        }
    }

    /// More Feedback rings longer; a key let go is silent 400 ms later.
    #[test]
    fn feedback_is_the_ring_and_the_key_mutes_the_string() {
        let level_late = |feedback: f32| {
            let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
            values[FEEDBACK] = feedback;
            let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                panic!("does not open")
            };
            let (l, _) = play(&mut inst, 48000, 256, &[(0, 0x90, 57, 100)]);
            peak(&l[36000..])
        };
        assert!(level_late(9990.0) > level_late(9000.0) * 5.0);
        let Some(mut inst) = open_native_instrument(ID, 48000, &[]) else {
            panic!("does not open")
        };
        let (l, _) = play(
            &mut inst,
            48000,
            256,
            &[(0, 0x90, 57, 100), (4800, 0x80, 57, 0)],
        );
        assert!(peak(&l[4800..9600]) > 0.01);
        assert_eq!(peak(&l[4800 + 19200 + 512..]), 0.0);
    }
}
