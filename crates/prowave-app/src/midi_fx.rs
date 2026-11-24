//! MIDI FX (Logic Pro: the MIDI FX slot of a channel strip): a chain of effects on a track
//! that changes its notes before they reach the instrument. They work on the notes the track
//! plays (after region and track transpose, quantize and limits), so the notes in the regions
//! stay as they were recorded; export writes the changed notes.
//!
//! - **Transposer** moves every note by semitones.
//! - **Velocity Processor** adds to the velocity and scales it.
//! - **Scale Quantizer** moves each note to the nearest note of a scale.
//! - **Chord Trigger** plays a chord on every note.
//! - **Note Repeater** repeats each note, at a rate, for as long as it is held.
//! - **Arpeggiator** plays the notes held together one after the other through a pattern of
//!   up to 32 steps (the generator is the engine's `arp.rs`, which also plays the keys played
//!   live; its patterns are in `arp_patterns.rs`).
//! - **Randomizer** shifts pitch, velocity and time by a random amount (the same every time).
//! - **Modulator** is an LFO that moves each note's velocity or pitch with its place in the
//!   cycle, or, for an External MIDI track, sends a controller (a filter, a volume) that follows
//!   the cycle all along the track.
use crate::pattern::StepRate;
use prowave_engine::{ArpMode, ArpRate, ArpSettings, ArpStep, NoteEvent, ARP_STEPS};

/// Most effects in a chain.
pub const MAX_FX: usize = 8;
/// Most notes a track's chain hands on: a repeater or arpeggiator over very many notes stops
/// here instead of using all the memory.
pub const MAX_FX_NOTES: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordKind {
    Major,
    Minor,
    Seventh,
    MajorSeventh,
    MinorSeventh,
    Sus4,
    Fifth,
    Octave,
}

impl ChordKind {
    pub const ALL: [ChordKind; 8] = [
        Self::Major,
        Self::Minor,
        Self::Seventh,
        Self::MajorSeventh,
        Self::MinorSeventh,
        Self::Sus4,
        Self::Fifth,
        Self::Octave,
    ];

    /// Semitones above the note, besides the note itself.
    pub fn intervals(self) -> &'static [i32] {
        match self {
            Self::Major => &[4, 7],
            Self::Minor => &[3, 7],
            Self::Seventh => &[4, 7, 10],
            Self::MajorSeventh => &[4, 7, 11],
            Self::MinorSeventh => &[3, 7, 10],
            Self::Sus4 => &[5, 7],
            Self::Fifth => &[7],
            Self::Octave => &[12],
        }
    }

    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0) as u8
    }

    pub fn from_index(i: u8) -> Self {
        Self::ALL.get(i as usize).copied().unwrap_or(Self::Major)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LfoShape {
    Sine,
    Triangle,
    Saw,
    Square,
    /// A new random value each cycle.
    Random,
}

impl LfoShape {
    pub const ALL: [LfoShape; 5] = [
        Self::Sine,
        Self::Triangle,
        Self::Saw,
        Self::Square,
        Self::Random,
    ];

    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0) as u8
    }

    pub fn from_index(i: u8) -> Self {
        Self::ALL.get(i as usize).copied().unwrap_or(Self::Sine)
    }

    /// The LFO at `phase` (0..1) of cycle number `cycle`, from -1 to 1.
    pub fn at(self, phase: f64, cycle: i64, seed: u32) -> f64 {
        let p = phase.rem_euclid(1.0);
        match self {
            Self::Sine => (p * std::f64::consts::TAU).sin(),
            Self::Triangle => {
                if p < 0.5 {
                    4.0 * p - 1.0
                } else {
                    3.0 - 4.0 * p
                }
            }
            Self::Saw => 2.0 * p - 1.0,
            Self::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Self::Random => f64::from(noise(seed, cycle as u64, 99)),
        }
    }
}

/// What a Modulator moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModTarget {
    /// Each note's velocity, up to 63 either way.
    Velocity,
    /// Each note's pitch, up to 12 semitones either way.
    Pitch,
    /// A MIDI controller sent to an External MIDI track, around the middle (64).
    Controller { number: u8 },
}

/// The lengths of a Modulator's cycle, in quarter notes.
pub const MOD_PERIODS: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];

/// A controller change a Modulator makes: beats from the start of the song.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModControl {
    pub beat: f64,
    pub number: u8,
    pub value: u8,
}

/// One effect and its settings.
#[derive(Clone, Debug, PartialEq)]
pub enum MidiFx {
    /// A program that changes the notes (see `scripter.rs`).
    Scripter {
        source: String,
    },
    Modulator {
        shape: LfoShape,
        /// Index into [`MOD_PERIODS`].
        period: u8,
        /// 0 - 100 percent of the full swing.
        depth: u8,
        target: ModTarget,
        seed: u32,
    },
    Transposer {
        semitones: i8,
    },
    Velocity {
        add: i8,
        /// 10 - 200 percent.
        scale: u8,
    },
    Scale {
        /// 0 = C.
        root: u8,
        minor: bool,
    },
    Chord {
        kind: ChordKind,
    },
    Repeater {
        rate: StepRate,
    },
    Arpeggiator(ArpSettings),
    Randomizer {
        /// Up to this many semitones either way.
        pitch: u8,
        /// Up to this much velocity either way.
        velocity: u8,
        /// 0 - 100: up to a sixteenth note early or late.
        timing: u8,
        seed: u32,
    },
    /// Swing on the off-beat sixteenths and a random spread of timing and velocity, as
    /// Hydrogen's song has them (see [`swing_and_humanize`]). Each is 0 to 100 percent.
    Swing {
        swing: u8,
        timing: u8,
        velocity: u8,
        seed: u32,
    },
    /// A held note played in a Euclidean rhythm: `pulses` hits spread as evenly as they go
    /// over `steps` steps of `rate`, turned by `rotation` steps (see [`euclidean_rhythm`]).
    Euclid {
        rate: StepRate,
        steps: u8,
        pulses: u8,
        rotation: i8,
    },
}

/// Most steps of a Euclidean rhythm.
pub const EUCLID_STEPS: u8 = 32;
/// The farthest Swing's Timing moves a note, in beats: Hydrogen's 2000 frames at 48 kHz
/// (41.7 ms), as beats at 120 beats a minute.
pub const HUMANIZE_BEATS: f64 = 2000.0 / 48000.0 * 2.0;
/// Hydrogen's standard deviations of the timing and the velocity it adds.
const HUMANIZE_TIMING_SD: f64 = 0.3;
const HUMANIZE_VELOCITY_SD: f64 = 0.2;

/// Which steps of a Euclidean rhythm are hits. Ported from Bespoke Synth's
/// `EuclideanSequencerRing::GetEuclideanRhythm` (Source/EuclideanSequencer.cpp, Copyright (C)
/// 2021 Ryan Challinor, GPL-3.0-or-later; BespokeSynth at commit 25b1541): a bucket that
/// overflows every `steps / pulses` steps, turned until a hit is first, then by `rotation`.
pub fn euclidean_rhythm(pulses: u8, steps: u8, rotation: i8) -> Vec<bool> {
    let n = usize::from(steps);
    let mut rhythm = vec![false; n];
    if pulses == 0 || steps == 0 {
        return rhythm;
    }
    let mut bucket = 0usize;
    for hit in rhythm.iter_mut() {
        bucket += usize::from(pulses);
        if bucket >= n {
            bucket -= n;
            *hit = true;
        }
    }
    if let Some(first) = rhythm.iter().position(|h| *h) {
        rhythm.rotate_left(first);
    }
    let turn = usize::from(rotation.unsigned_abs()) % n;
    if rotation >= 0 {
        rhythm.rotate_left(turn);
    } else {
        rhythm.rotate_right(turn);
    }
    rhythm
}

/// A number from a normal distribution of standard deviation 1, the same every time for the
/// same `seed`, `a` and `b` (Box and Muller's method on two of [`noise`]'s numbers).
fn gaussian(seed: u32, a: u64, b: u64) -> f64 {
    let u1 = (f64::from(noise(seed, a, b)) * 0.5 + 0.5).clamp(1e-9, 1.0);
    let u2 = f64::from(noise(seed ^ 0x5bd1_e995, a, b)) * 0.5 + 0.5;
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// Hydrogen's swing and humanize on one note. Ported from Hydrogen's `Note::swing`,
/// `Note::humanize` and the swing test of its audio engine's note queue
/// (src/core/Basics/Note.cpp, src/core/AudioEngine/AudioEngine.cpp; Copyright (c) 2002-2008
/// Alex "Comix" Cominu, 2008-2026 the hydrogen development team; GPL-2.0-or-later, taken here
/// under GPL-3.0-or-later; hydrogen-music/hydrogen at commit 00509be). A note that starts on
/// an off-beat sixteenth is delayed by `swing` of a thirty-second note; every note's start
/// moves by a normal random amount (never more than [`HUMANIZE_BEATS`]) and its velocity by
/// another. Hydrogen counts the timing in frames; here it is beats, as at 120 beats a minute.
fn swing_and_humanize(
    n: NoteEvent,
    i: usize,
    swing: u8,
    timing: u8,
    velocity: u8,
    seed: u32,
) -> NoteEvent {
    let key = (n.start_beat * 960.0).round() as u64;
    let id = i as u64 ^ (u64::from(n.pitch) << 40);
    let mut delay = 0.0;
    // On a sixteenth, not on an eighth.
    let sixteenths = n.start_beat * 4.0;
    let on_grid = (sixteenths - sixteenths.round()).abs() < 1e-6;
    if on_grid && (sixteenths.round() as i64).rem_euclid(2) == 1 {
        delay += 0.125 * f64::from(swing) / 100.0;
    }
    if timing > 0 {
        let amount = f64::from(timing) / 100.0 * HUMANIZE_BEATS;
        delay += (amount * gaussian(seed, key, id) * HUMANIZE_TIMING_SD)
            .clamp(-HUMANIZE_BEATS, HUMANIZE_BEATS);
    }
    let start = (n.start_beat + delay).max(0.0);
    let mut vel = f64::from(n.vel) / 127.0;
    if velocity > 0 {
        vel += f64::from(velocity) / 100.0 * gaussian(seed ^ 1, key, id) * HUMANIZE_VELOCITY_SD;
    }
    NoteEvent {
        start_beat: start,
        end_beat: (n.end_beat + (start - n.start_beat)).max(start + 1e-3),
        vel: (vel * 127.0).round().clamp(1.0, 127.0) as u8,
        ..n
    }
}

impl MidiFx {
    /// Every kind with its settings at their start values, in the order a menu lists them.
    pub fn all_defaults() -> [MidiFx; 11] {
        [
            Self::Scripter {
                source: crate::scripter::DEFAULT_SCRIPT.to_string(),
            },
            Self::Modulator {
                shape: LfoShape::Sine,
                period: 3,
                depth: 50,
                target: ModTarget::Velocity,
                seed: 1,
            },
            Self::Transposer { semitones: 0 },
            Self::Velocity { add: 0, scale: 100 },
            Self::Scale {
                root: 0,
                minor: false,
            },
            Self::Chord {
                kind: ChordKind::Major,
            },
            Self::Repeater {
                rate: StepRate::Sixteenth,
            },
            Self::Arpeggiator(ArpSettings::default()),
            Self::Randomizer {
                pitch: 0,
                velocity: 10,
                timing: 0,
                seed: 1,
            },
            Self::Swing {
                swing: 50,
                timing: 0,
                velocity: 0,
                seed: 1,
            },
            Self::Euclid {
                rate: StepRate::Sixteenth,
                steps: 8,
                pulses: 3,
                rotation: 0,
            },
        ]
    }

    /// How it is written in the project file.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Scripter { .. } => "scripter",
            Self::Modulator { .. } => "modulator",
            Self::Transposer { .. } => "transposer",
            Self::Velocity { .. } => "velocity",
            Self::Scale { .. } => "scale",
            Self::Chord { .. } => "chord",
            Self::Repeater { .. } => "repeater",
            // "arpeggiator" was the one before the step pattern: four numbers (see `old_arp`).
            Self::Arpeggiator(_) => "step_arpeggiator",
            Self::Randomizer { .. } => "randomizer",
            Self::Swing { .. } => "swing",
            Self::Euclid { .. } => "euclid",
        }
    }

    /// The settings as numbers, for the project file.
    pub fn numbers(&self) -> Vec<f32> {
        if let Self::Arpeggiator(a) = self {
            return arp_numbers(a);
        }
        match *self {
            Self::Scripter { .. } => Vec::new(),
            Self::Modulator {
                shape,
                period,
                depth,
                target,
                seed,
            } => {
                let (kind, number) = match target {
                    ModTarget::Velocity => (0u8, 0u8),
                    ModTarget::Pitch => (1u8, 0u8),
                    ModTarget::Controller { number } => (2u8, number),
                };
                vec![
                    f32::from(shape.index()),
                    f32::from(period),
                    f32::from(depth),
                    f32::from(kind),
                    f32::from(number),
                    seed as f32,
                ]
            }
            Self::Transposer { semitones } => vec![f32::from(semitones)],
            Self::Velocity { add, scale } => vec![f32::from(add), f32::from(scale)],
            Self::Scale { root, minor } => vec![f32::from(root), f32::from(u8::from(minor))],
            Self::Chord { kind } => vec![f32::from(kind.index())],
            Self::Repeater { rate } => vec![rate_index(rate) as f32],
            Self::Arpeggiator(_) => Vec::new(),
            Self::Randomizer {
                pitch,
                velocity,
                timing,
                seed,
            } => vec![
                f32::from(pitch),
                f32::from(velocity),
                f32::from(timing),
                seed as f32,
            ],
            Self::Swing {
                swing,
                timing,
                velocity,
                seed,
            } => vec![
                f32::from(swing),
                f32::from(timing),
                f32::from(velocity),
                seed as f32,
            ],
            Self::Euclid {
                rate,
                steps,
                pulses,
                rotation,
            } => vec![
                rate_index(rate) as f32,
                f32::from(steps),
                f32::from(pulses),
                f32::from(rotation),
            ],
        }
    }

    /// The effect called `name` with settings `n` (missing ones start values, wild ones
    /// repaired), if there is one.
    pub fn from_numbers(name: &str, n: &[f32]) -> Option<MidiFx> {
        Self::from_parts(name, n, "")
    }

    /// [`MidiFx::from_numbers`] for a file's effect: the scripter's script comes along.
    pub fn from_parts(name: &str, n: &[f32], script: &str) -> Option<MidiFx> {
        let get = |i: usize, default: f32| {
            n.get(i)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or(default)
        };
        let int = |i: usize, default: f32, lo: f32, hi: f32| get(i, default).round().clamp(lo, hi);
        let fx = match name {
            "scripter" => Self::Scripter {
                source: script
                    .chars()
                    .take(crate::scripter::MAX_SCRIPT_CHARS)
                    .collect(),
            },
            "modulator" => Self::Modulator {
                shape: LfoShape::from_index(int(0, 0.0, 0.0, 255.0) as u8),
                period: int(1, 3.0, 0.0, (MOD_PERIODS.len() - 1) as f32) as u8,
                depth: int(2, 50.0, 0.0, 100.0) as u8,
                target: match int(3, 0.0, 0.0, 2.0) as u8 {
                    0 => ModTarget::Velocity,
                    1 => ModTarget::Pitch,
                    _ => ModTarget::Controller {
                        number: int(4, 1.0, 0.0, 127.0) as u8,
                    },
                },
                seed: get(5, 1.0).max(0.0) as u32,
            },
            "transposer" => Self::Transposer {
                semitones: int(0, 0.0, -48.0, 48.0) as i8,
            },
            "velocity" => Self::Velocity {
                add: int(0, 0.0, -127.0, 127.0) as i8,
                scale: int(1, 100.0, 10.0, 200.0) as u8,
            },
            "scale" => Self::Scale {
                root: int(0, 0.0, 0.0, 11.0) as u8,
                minor: get(1, 0.0) >= 0.5,
            },
            "chord" => Self::Chord {
                kind: ChordKind::from_index(int(0, 0.0, 0.0, 255.0) as u8),
            },
            "repeater" => Self::Repeater {
                rate: rate_of_index(int(0, 2.0, 0.0, 255.0) as usize),
            },
            "step_arpeggiator" => Self::Arpeggiator(arp_of_numbers(n)),
            "arpeggiator" => Self::Arpeggiator(old_arp(
                rate_of_index(int(0, 2.0, 0.0, 255.0) as usize),
                int(1, 0.0, 0.0, 255.0) as u8,
                int(2, 1.0, 1.0, 4.0) as u8,
                int(3, 80.0, 5.0, 100.0) as u8,
            )),
            "randomizer" => Self::Randomizer {
                pitch: int(0, 0.0, 0.0, 24.0) as u8,
                velocity: int(1, 10.0, 0.0, 127.0) as u8,
                timing: int(2, 0.0, 0.0, 100.0) as u8,
                seed: get(3, 1.0).max(0.0) as u32,
            },
            "swing" => Self::Swing {
                swing: int(0, 50.0, 0.0, 100.0) as u8,
                timing: int(1, 0.0, 0.0, 100.0) as u8,
                velocity: int(2, 0.0, 0.0, 100.0) as u8,
                seed: get(3, 1.0).max(0.0) as u32,
            },
            "euclid" => {
                let steps = int(1, 8.0, 1.0, f32::from(EUCLID_STEPS)) as u8;
                Self::Euclid {
                    rate: rate_of_index(int(0, 2.0, 0.0, 255.0) as usize),
                    steps,
                    pulses: int(2, 3.0, 0.0, f32::from(steps)) as u8,
                    rotation: int(3, 0.0, -31.0, 31.0) as i8,
                }
            }
            _ => return None,
        };
        Some(fx)
    }
}

/// The numbers before the steps in an Arpeggiator's settings, and the numbers of a step.
const ARP_HEAD: usize = 10;
const ARP_STEP: usize = 5;

/// An Arpeggiator's settings as numbers: rate, mode, octaves, gate, swing, step velocity,
/// latch, song position, length, seed, then for each of the 32 steps on, velocity, tie,
/// chord, transpose.
fn arp_numbers(a: &ArpSettings) -> Vec<f32> {
    let flag = |b: bool| f32::from(u8::from(b));
    let mut n = vec![
        f32::from(a.rate.index()),
        f32::from(a.mode.index()),
        f32::from(a.octaves),
        f32::from(a.gate),
        f32::from(a.swing),
        f32::from(a.step_velocity),
        flag(a.latch),
        flag(a.sync),
        f32::from(a.length),
        a.seed as f32,
    ];
    for s in &a.steps {
        n.extend([
            flag(s.on),
            f32::from(s.vel),
            flag(s.tie),
            flag(s.chord),
            f32::from(s.transpose),
        ]);
    }
    n
}

/// The Arpeggiator of numbers `n` (missing ones start values, wild ones repaired).
fn arp_of_numbers(n: &[f32]) -> ArpSettings {
    let d = ArpSettings::default();
    let get = |i: usize, default: f32| {
        n.get(i)
            .copied()
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    };
    let int = |i: usize, default: f32, lo: f32, hi: f32| get(i, default).round().clamp(lo, hi);
    let flag = |i: usize, default: bool| get(i, f32::from(u8::from(default))) >= 0.5;
    let mut a = ArpSettings {
        rate: ArpRate::from_index(int(0, f32::from(d.rate.index()), 0.0, 255.0) as u8),
        mode: ArpMode::from_index(int(1, 0.0, 0.0, 255.0) as u8),
        octaves: int(2, 1.0, 1.0, 4.0) as u8,
        gate: int(3, 80.0, 5.0, 200.0) as u8,
        swing: int(4, 0.0, 0.0, 100.0) as u8,
        step_velocity: int(5, 100.0, 0.0, 100.0) as u8,
        latch: flag(6, false),
        sync: flag(7, false),
        length: int(8, 16.0, 1.0, ARP_STEPS as f32) as u8,
        seed: get(9, 1.0).clamp(0.0, 16_777_215.0) as u32,
        steps: d.steps,
    };
    for (k, s) in a.steps.iter_mut().enumerate() {
        let at = ARP_HEAD + k * ARP_STEP;
        *s = ArpStep {
            on: flag(at, true),
            vel: int(at + 1, 100.0, 1.0, 127.0) as u8,
            tie: flag(at + 2, false),
            chord: flag(at + 3, false),
            transpose: int(at + 4, 0.0, -24.0, 24.0) as i8,
        };
    }
    a
}

/// The Arpeggiator a project from before the step pattern had (a rate, one of five orders,
/// octaves and a gate), as the same thing with every step on: each chord starts it again and
/// the notes keep the velocity they were played with.
fn old_arp(rate: StepRate, pattern: u8, octaves: u8, gate: u8) -> ArpSettings {
    ArpSettings {
        rate: match rate {
            StepRate::Quarter => ArpRate::Quarter,
            StepRate::Eighth => ArpRate::Eighth,
            StepRate::Sixteenth => ArpRate::Sixteenth,
            StepRate::ThirtySecond => ArpRate::ThirtySecond,
            StepRate::EighthTriplet => ArpRate::EighthTriplet,
            StepRate::SixteenthTriplet => ArpRate::SixteenthTriplet,
        },
        mode: match pattern {
            1 => ArpMode::Down,
            2 => ArpMode::UpDown,
            3 => ArpMode::Random,
            4 => ArpMode::AsPlayed,
            _ => ArpMode::Up,
        },
        octaves,
        gate,
        step_velocity: 0,
        ..ArpSettings::default()
    }
}

/// The Arpeggiator the keys played live on a track go through: the first one that is on.
pub fn live_arp(chain: &[FxSlot]) -> Option<ArpSettings> {
    chain.iter().filter(|s| s.on).find_map(|s| match &s.fx {
        MidiFx::Arpeggiator(a) => Some(*a),
        _ => None,
    })
}

fn rate_index(rate: StepRate) -> usize {
    StepRate::ALL.iter().position(|r| *r == rate).unwrap_or(2)
}

fn rate_of_index(i: usize) -> StepRate {
    StepRate::ALL.get(i).copied().unwrap_or(StepRate::Sixteenth)
}

/// An effect in a chain, which can be switched off without losing its settings.
#[derive(Clone, Debug, PartialEq)]
pub struct FxSlot {
    pub fx: MidiFx,
    pub on: bool,
}

/// A small generator for the same numbers every time.
fn noise(seed: u32, a: u64, b: u64) -> f32 {
    let mut x = u64::from(seed)
        ^ a.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ b.wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    (x >> 40) as f32 / 16_777_216.0 * 2.0 - 1.0
}

const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

/// `pitch` moved to the nearest note of the scale (down first when two are as near).
pub fn in_scale(pitch: u8, root: u8, minor: bool) -> u8 {
    let scale = if minor { &MINOR } else { &MAJOR };
    let mut best = pitch;
    let mut best_d = i32::MAX;
    for delta in 0..=6i32 {
        for sign in [-1, 1] {
            let p = i32::from(pitch) + sign * delta;
            if !(0..=127).contains(&p) {
                continue;
            }
            let degree = ((p - i32::from(root)).rem_euclid(12)) as u8;
            if scale.contains(&degree) && delta < best_d {
                best_d = delta;
                best = p as u8;
            }
        }
        if best_d != i32::MAX {
            break;
        }
    }
    best
}

fn clamp_pitch(p: i32) -> Option<u8> {
    (0..=127).contains(&p).then_some(p as u8)
}

/// Runs `notes` (absolute beats, any order) through the chain; the result is sorted by start.
#[cfg(test)]
pub fn apply(chain: &[FxSlot], notes: Vec<NoteEvent>) -> Vec<NoteEvent> {
    let end = notes.iter().map(|n| n.end_beat).fold(0.0, f64::max);
    apply_until(chain, notes, end)
}

/// Runs `notes` (absolute beats, any order) of a track whose regions end at beat `end` through
/// the chain (a latched Arpeggiator plays on until there); the result is sorted by start.
pub fn apply_until(chain: &[FxSlot], mut notes: Vec<NoteEvent>, end: f64) -> Vec<NoteEvent> {
    for slot in chain.iter().filter(|s| s.on) {
        notes = run(&slot.fx, notes, end);
        notes.truncate(MAX_FX_NOTES);
    }
    notes.sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
    notes
}

/// The LFO of a Modulator at `beat`, from -1 to 1.
fn lfo(shape: LfoShape, period: u8, seed: u32, beat: f64) -> f64 {
    let len = MOD_PERIODS[(period as usize).min(MOD_PERIODS.len() - 1)];
    let cycles = beat / len;
    shape.at(cycles, cycles.floor() as i64, seed)
}

/// The controller changes the Modulators of `chain` make between the beats `from` and `to`: a
/// change each time the controller's value moves, at the smallest every sixteenth of a beat.
pub fn controls(chain: &[FxSlot], from: f64, to: f64) -> Vec<ModControl> {
    let mut out = Vec::new();
    if !(from.is_finite() && to.is_finite()) || to <= from {
        return out;
    }
    for slot in chain.iter().filter(|s| s.on) {
        let MidiFx::Modulator {
            shape,
            period,
            depth,
            target: ModTarget::Controller { number },
            seed,
        } = slot.fx
        else {
            continue;
        };
        let len = MOD_PERIODS[(period as usize).min(MOD_PERIODS.len() - 1)];
        let step = (len / 16.0).max(1.0 / 16.0);
        let mut last: Option<u8> = None;
        let mut beat = from;
        let mut count = 0usize;
        while beat <= to && count < MAX_FX_NOTES {
            let swing = lfo(shape, period, seed, beat) * f64::from(depth) / 100.0 * 63.0;
            let value = (64.0 + swing).round().clamp(0.0, 127.0) as u8;
            if last != Some(value) {
                out.push(ModControl {
                    beat,
                    number,
                    value,
                });
                last = Some(value);
                count += 1;
            }
            beat += step;
        }
    }
    out.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    out
}

fn run(fx: &MidiFx, notes: Vec<NoteEvent>, end: f64) -> Vec<NoteEvent> {
    match *fx {
        // A script that fails leaves the notes as they were (the window says what is wrong).
        MidiFx::Scripter { ref source } => crate::scripter::run(source, &notes).unwrap_or(notes),
        MidiFx::Modulator {
            shape,
            period,
            depth,
            target,
            seed,
        } => notes
            .into_iter()
            .filter_map(|n| {
                let x = lfo(shape, period, seed, n.start_beat) * f64::from(depth) / 100.0;
                match target {
                    ModTarget::Velocity => Some(NoteEvent {
                        vel: (f64::from(n.vel) + x * 63.0).round().clamp(1.0, 127.0) as u8,
                        ..n
                    }),
                    ModTarget::Pitch => Some(NoteEvent {
                        pitch: clamp_pitch(i32::from(n.pitch) + (x * 12.0).round() as i32)?,
                        ..n
                    }),
                    // A controller does not change the notes.
                    ModTarget::Controller { .. } => Some(n),
                }
            })
            .collect(),
        MidiFx::Transposer { semitones } => notes
            .into_iter()
            .filter_map(|n| {
                let pitch = clamp_pitch(i32::from(n.pitch) + i32::from(semitones))?;
                Some(NoteEvent { pitch, ..n })
            })
            .collect(),
        MidiFx::Velocity { add, scale } => notes
            .into_iter()
            .map(|n| {
                let v =
                    (f32::from(n.vel) * f32::from(scale) / 100.0).round() as i32 + i32::from(add);
                NoteEvent {
                    vel: v.clamp(1, 127) as u8,
                    ..n
                }
            })
            .collect(),
        MidiFx::Scale { root, minor } => notes
            .into_iter()
            .map(|n| NoteEvent {
                pitch: in_scale(n.pitch, root, minor),
                ..n
            })
            .collect(),
        MidiFx::Chord { kind } => {
            let mut out = Vec::with_capacity((notes.len() * 3).min(MAX_FX_NOTES));
            for n in notes {
                if out.len() >= MAX_FX_NOTES {
                    break;
                }
                out.push(n);
                for &i in kind.intervals() {
                    if let Some(pitch) = clamp_pitch(i32::from(n.pitch) + i) {
                        out.push(NoteEvent { pitch, ..n });
                    }
                }
            }
            out
        }
        MidiFx::Repeater { rate } => {
            let step = rate.beats();
            let mut out = Vec::new();
            for n in notes {
                if out.len() >= MAX_FX_NOTES {
                    break;
                }
                let mut at = n.start_beat;
                let mut count = 0;
                while at < n.end_beat - 1e-6 && count < 512 {
                    out.push(NoteEvent {
                        start_beat: at,
                        end_beat: (at + step * 0.9).min(n.end_beat),
                        ..n
                    });
                    at += step;
                    count += 1;
                }
            }
            out
        }
        MidiFx::Arpeggiator(ref arp) => arpeggiate(notes, arp, end),
        MidiFx::Swing {
            swing,
            timing,
            velocity,
            seed,
        } => notes
            .into_iter()
            .enumerate()
            .map(|(i, n)| swing_and_humanize(n, i, swing, timing, velocity, seed))
            .collect(),
        MidiFx::Euclid {
            rate,
            steps,
            pulses,
            rotation,
        } => {
            let step = rate.beats();
            let steps = steps.clamp(1, EUCLID_STEPS);
            let rhythm = euclidean_rhythm(pulses.min(steps), steps, rotation);
            let mut out = Vec::new();
            for n in notes {
                // The pattern is laid on the song's grid, so every note keeps the same time.
                let mut k = (n.start_beat / step - 1e-9).ceil().max(0.0) as u64;
                let mut count = 0;
                while (k as f64) * step < n.end_beat - 1e-6 && count < 4096 {
                    if out.len() >= MAX_FX_NOTES {
                        break;
                    }
                    let at = k as f64 * step;
                    if rhythm[(k % rhythm.len() as u64) as usize] {
                        out.push(NoteEvent {
                            start_beat: at,
                            end_beat: (at + step * 0.9).min(n.end_beat),
                            ..n
                        });
                    }
                    k += 1;
                    count += 1;
                }
            }
            out
        }
        MidiFx::Randomizer {
            pitch,
            velocity,
            timing,
            seed,
        } => notes
            .into_iter()
            .enumerate()
            .filter_map(|(i, n)| {
                let key = (n.start_beat * 960.0) as u64;
                let id = i as u64 ^ (u64::from(n.pitch) << 40);
                let dp = (noise(seed, key, id) * (f32::from(pitch) + 0.999)).trunc() as i32;
                let dv = (noise(seed ^ 1, key, id) * (f32::from(velocity) + 0.999)).trunc() as i32;
                let dt = f64::from(noise(seed ^ 2, key, id)) * f64::from(timing) / 100.0 * 0.25;
                let p = clamp_pitch(i32::from(n.pitch) + dp)?;
                let start = (n.start_beat + dt).max(0.0);
                Some(NoteEvent {
                    start_beat: start,
                    end_beat: (n.end_beat + (start - n.start_beat)).max(start + 1e-3),
                    pitch: p,
                    vel: (i32::from(n.vel) + dv).clamp(1, 127) as u8,
                    ..n
                })
            })
            .collect(),
    }
}

/// The notes of each track played through the Arpeggiator (the notes held together are its
/// keys), with the engine's generator, so the regions sound as the same keys played live do.
fn arpeggiate(notes: Vec<NoteEvent>, arp: &ArpSettings, end: f64) -> Vec<NoteEvent> {
    let mut tracks: Vec<u16> = notes.iter().map(|n| n.track).collect();
    tracks.sort_unstable();
    tracks.dedup();
    let mut out = Vec::new();
    for track in tracks {
        let own: Vec<NoteEvent> = notes.iter().filter(|n| n.track == track).copied().collect();
        let room = MAX_FX_NOTES.saturating_sub(out.len());
        out.extend(prowave_engine::arp::render(arp, &own, end, room));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(start: f64, end: f64, pitch: u8) -> NoteEvent {
        NoteEvent {
            start_beat: start,
            end_beat: end,
            pitch,
            vel: 100,
            track: 3,
        }
    }

    fn slot(fx: MidiFx) -> FxSlot {
        FxSlot { fx, on: true }
    }

    fn pitches(notes: &[NoteEvent]) -> Vec<u8> {
        notes.iter().map(|n| n.pitch).collect()
    }

    #[test]
    fn transposer_and_velocity_move_notes_and_drop_what_leaves_the_range() {
        let notes = vec![note(0.0, 1.0, 60), note(1.0, 2.0, 120)];
        let up = apply(&[slot(MidiFx::Transposer { semitones: 12 })], notes.clone());
        assert_eq!(pitches(&up), [72], "127 is the top");
        let down = apply(
            &[slot(MidiFx::Transposer { semitones: -12 })],
            notes.clone(),
        );
        assert_eq!(pitches(&down), [48, 108]);
        let v = apply(
            &[slot(MidiFx::Velocity { add: 20, scale: 50 })],
            notes.clone(),
        );
        assert_eq!(v[0].vel, 70);
        let hard = apply(
            &[slot(MidiFx::Velocity {
                add: 100,
                scale: 200,
            })],
            notes,
        );
        assert_eq!(hard[0].vel, 127);
    }

    #[test]
    fn a_switched_off_effect_does_nothing_and_a_chain_runs_in_order() {
        let notes = vec![note(0.0, 1.0, 60)];
        let off = FxSlot {
            fx: MidiFx::Transposer { semitones: 5 },
            on: false,
        };
        assert_eq!(apply(&[off], notes.clone()), notes);
        // Chord then transpose: the whole chord moves. Transpose then scale: a note off the
        // scale is pulled in afterwards.
        let chain = [
            slot(MidiFx::Chord {
                kind: ChordKind::Major,
            }),
            slot(MidiFx::Transposer { semitones: 2 }),
        ];
        assert_eq!(pitches(&apply(&chain, notes.clone())), [62, 66, 69]);
        let chain = [
            slot(MidiFx::Transposer { semitones: 1 }),
            slot(MidiFx::Scale {
                root: 0,
                minor: false,
            }),
        ];
        assert_eq!(
            pitches(&apply(&chain, notes)),
            [60],
            "61 is not in C major: down to 60"
        );
    }

    #[test]
    fn the_scale_quantizer_picks_the_nearest_note_of_the_scale() {
        // C major: black keys go down (the nearer white key is a tie, so the lower one).
        let major: Vec<u8> = (60..72).map(|p| in_scale(p, 0, false)).collect();
        assert_eq!(major, [60, 60, 62, 62, 64, 65, 65, 67, 67, 69, 69, 71]);
        // A minor from root 9 has the same notes as C major; C minor has Eb.
        assert_eq!(in_scale(63, 0, true), 63);
        assert_eq!(in_scale(64, 0, true), 63);
        assert_eq!(in_scale(61, 9, true), 60, "A minor, 61 -> 60");
        assert!((0..128).all(|p| in_scale(p, 5, false) <= 127));
    }

    #[test]
    fn a_chord_trigger_adds_the_chord_on_every_note() {
        let notes = vec![note(0.0, 1.0, 60), note(2.0, 3.0, 125)];
        for kind in ChordKind::ALL {
            let out = apply(&[slot(MidiFx::Chord { kind })], notes.clone());
            assert!(out.len() >= 2 + kind.intervals().len() - 1, "{kind:?}");
            assert!(out.iter().all(|n| n.pitch <= 127 && n.track == 3));
        }
        let m = apply(
            &[slot(MidiFx::Chord {
                kind: ChordKind::MinorSeventh,
            })],
            notes,
        );
        assert_eq!(pitches(&m[..4]), [60, 63, 67, 70]);
        assert!(
            m[4..].iter().all(|n| n.start_beat == 2.0) && m.len() == 4 + 1,
            "the top of 125 is cut"
        );
    }

    #[test]
    fn the_repeater_repeats_a_held_note_at_its_rate() {
        let out = apply(
            &[slot(MidiFx::Repeater {
                rate: StepRate::Eighth,
            })],
            vec![note(1.0, 3.0, 60), note(4.0, 4.2, 62)],
        );
        let starts: Vec<f64> = out.iter().map(|n| n.start_beat).collect();
        assert_eq!(starts, [1.0, 1.5, 2.0, 2.5, 4.0]);
        assert!(out.iter().all(|n| n.end_beat - n.start_beat <= 0.45 + 1e-9));
        assert!(out
            .iter()
            .all(|n| n.end_beat <= 4.2 + 1e-9 || n.start_beat < 4.0));
    }

    /// The Arpeggiator of a project from before the step pattern: rate (0 quarter, 1 eighth),
    /// order (0 up, 1 down, 2 up and down, 3 random), octaves, gate.
    fn old(rate: f32, order: f32, octaves: f32, gate: f32) -> FxSlot {
        slot(MidiFx::from_numbers("arpeggiator", &[rate, order, octaves, gate]).unwrap())
    }

    #[test]
    fn an_arpeggiator_from_an_older_project_plays_what_it_played() {
        let chord = vec![note(0.0, 4.0, 67), note(0.0, 4.0, 60), note(0.0, 4.0, 64)];
        let arp = |order: f32| apply(&[old(0.0, order, 1.0, 50.0)], chord.clone());
        assert_eq!(pitches(&arp(0.0)), [60, 64, 67, 60]);
        assert!(arp(0.0).iter().all(|n| n.vel == 100 && n.track == 3));
        let eighths = apply(&[old(1.0, 0.0, 1.0, 100.0)], chord.clone());
        assert_eq!(pitches(&eighths), [60, 64, 67, 60, 64, 67, 60, 64]);
        assert_eq!(eighths[1].start_beat, 0.5);
        assert!((eighths[0].end_beat - 0.5).abs() < 1e-9);
        let two = apply(&[old(1.0, 1.0, 2.0, 100.0)], chord.clone());
        assert_eq!(pitches(&two[..6]), [79, 76, 72, 67, 64, 60]);
        let updown = apply(&[old(1.0, 2.0, 1.0, 100.0)], chord.clone());
        assert_eq!(pitches(&updown[..6]), [60, 64, 67, 64, 60, 64]);
        // Random is the same notes, the same every time.
        let r1 = pitches(&arp(3.0));
        assert_eq!(r1, pitches(&arp(3.0)));
        assert!(r1.iter().all(|p| [60, 64, 67].contains(p)));
        // Two chords one after the other each get their own run, a single note is repeated.
        let two_chords = apply(
            &[old(0.0, 0.0, 1.0, 100.0)],
            vec![note(0.0, 2.0, 60), note(0.0, 2.0, 64), note(2.0, 4.0, 62)],
        );
        assert_eq!(pitches(&two_chords), [60, 64, 62, 62]);
        assert!(apply(&[old(0.0, 0.0, 1.0, 50.0)], vec![]).is_empty());
    }

    #[test]
    fn the_arpeggiator_plays_its_step_pattern_on_each_track_and_in_a_chain() {
        let mut arp = ArpSettings {
            rate: ArpRate::Quarter,
            length: 2,
            ..ArpSettings::default()
        };
        arp.steps[1].transpose = 12;
        arp.steps[1].vel = 40;
        let chord = vec![note(0.0, 4.0, 64), note(0.0, 4.0, 60)];
        let out = apply(&[slot(MidiFx::Arpeggiator(arp))], chord.clone());
        assert_eq!(pitches(&out), [60, 76, 60, 76]);
        assert_eq!(
            out.iter().map(|n| n.vel).collect::<Vec<_>>(),
            [100, 40, 100, 40]
        );
        // Two tracks' notes are two keyboards.
        let mut both = chord.clone();
        both.push(NoteEvent {
            track: 9,
            ..note(1.0, 3.0, 30)
        });
        let out = apply(&[slot(MidiFx::Arpeggiator(arp))], both);
        let of = |t: u16| {
            out.iter()
                .filter(|n| n.track == t)
                .map(|n| (n.start_beat, n.pitch))
                .collect::<Vec<_>>()
        };
        assert_eq!(of(3), [(0.0, 60), (1.0, 76), (2.0, 60), (3.0, 76)]);
        assert_eq!(of(9), [(1.0, 30), (2.0, 42)]);
        // A Chord Trigger before it gives it a chord to play; a Transposer after it moves
        // what it made.
        let chain = [
            slot(MidiFx::Chord {
                kind: ChordKind::Fifth,
            }),
            slot(MidiFx::Arpeggiator(ArpSettings {
                rate: ArpRate::Quarter,
                ..ArpSettings::default()
            })),
            slot(MidiFx::Transposer { semitones: 1 }),
        ];
        assert_eq!(
            pitches(&apply(&chain, vec![note(0.0, 3.0, 60)])),
            [61, 68, 61]
        );
        // Latched, the last chord plays on to the end of the track's regions.
        arp.latch = true;
        let short = vec![note(0.0, 0.5, 60)];
        let held = apply_until(&[slot(MidiFx::Arpeggiator(arp))], short.clone(), 4.0);
        assert_eq!(pitches(&held), [60, 72, 60, 72]);
        assert_eq!(apply(&[slot(MidiFx::Arpeggiator(arp))], short).len(), 1);
        // The keys played live go through the first Arpeggiator that is on.
        let off = FxSlot {
            fx: MidiFx::Arpeggiator(ArpSettings::default()),
            on: false,
        };
        assert_eq!(live_arp(std::slice::from_ref(&off)), None);
        assert_eq!(
            live_arp(&[
                off,
                slot(MidiFx::Transposer { semitones: 1 }),
                slot(MidiFx::Arpeggiator(arp))
            ]),
            Some(arp)
        );
    }

    #[test]
    fn the_randomizer_is_random_but_the_same_every_time_and_within_its_limits() {
        let notes: Vec<NoteEvent> = (0..40)
            .map(|i| note(i as f64, i as f64 + 0.5, 60))
            .collect();
        let fx = MidiFx::Randomizer {
            pitch: 3,
            velocity: 20,
            timing: 50,
            seed: 5,
        };
        let a = apply(&[slot(fx.clone())], notes.clone());
        assert_eq!(a, apply(&[slot(fx)], notes.clone()));
        assert_eq!(a.len(), 40);
        assert!(a.iter().all(|n| (57..=63).contains(&n.pitch)));
        assert!(a.iter().all(|n| (80..=120).contains(&n.vel)));
        assert!(a.iter().any(|n| n.pitch != 60) && a.iter().any(|n| n.vel != 100));
        assert!(a
            .iter()
            .all(|n| n.end_beat > n.start_beat && n.start_beat >= 0.0));
        let moved = a
            .iter()
            .zip(&notes)
            .map(|(x, y)| (x.start_beat - y.start_beat).abs())
            .fold(0.0, f64::max);
        assert!(moved > 0.01 && moved <= 0.125 + 1e-9, "{moved}");
        // With nothing to randomise, nothing changes.
        let none = MidiFx::Randomizer {
            pitch: 0,
            velocity: 0,
            timing: 0,
            seed: 1,
        };
        assert_eq!(apply(&[slot(none)], notes.clone()), notes);
    }

    #[test]
    fn the_lfo_shapes_swing_between_minus_one_and_one() {
        for shape in LfoShape::ALL {
            let values: Vec<f64> = (0..64).map(|i| shape.at(i as f64 / 64.0, 0, 3)).collect();
            assert!(values.iter().all(|v| (-1.0..=1.0).contains(v)), "{shape:?}");
            assert_eq!(LfoShape::from_index(shape.index()), shape);
        }
        assert!((LfoShape::Sine.at(0.25, 0, 0) - 1.0).abs() < 1e-12);
        assert!(LfoShape::Sine.at(0.0, 0, 0).abs() < 1e-12);
        assert_eq!(LfoShape::Triangle.at(0.0, 0, 0), -1.0);
        assert_eq!(LfoShape::Triangle.at(0.5, 0, 0), 1.0);
        assert_eq!(LfoShape::Saw.at(0.0, 0, 0), -1.0);
        assert_eq!(LfoShape::Square.at(0.1, 0, 0), 1.0);
        assert_eq!(LfoShape::Square.at(0.6, 0, 0), -1.0);
        // Random holds a value for a cycle and differs from cycle to cycle.
        let a = LfoShape::Random.at(0.2, 5, 1);
        assert_eq!(a, LfoShape::Random.at(0.9, 5, 1));
        assert_ne!(a, LfoShape::Random.at(0.2, 6, 1));
        // Phases before the start work too.
        assert!(LfoShape::Sine.at(-0.25, -1, 0) < 0.0);
    }

    #[test]
    fn a_modulator_moves_velocity_or_pitch_with_the_cycle() {
        // A sine of 2 beats: 0 at beat 0, the top at 0.5, 0 at 1, the bottom at 1.5.
        let notes = vec![
            note(0.0, 0.2, 60),
            note(0.5, 0.7, 60),
            note(1.0, 1.2, 60),
            note(1.5, 1.7, 60),
        ];
        let fx = |target| {
            slot(MidiFx::Modulator {
                shape: LfoShape::Sine,
                period: 3,
                depth: 100,
                target,
                seed: 1,
            })
        };
        let v = apply(&[fx(ModTarget::Velocity)], notes.clone());
        assert_eq!(
            v.iter().map(|n| n.vel).collect::<Vec<_>>(),
            [100, 127, 100, 37]
        );
        let p = apply(&[fx(ModTarget::Pitch)], notes.clone());
        assert_eq!(pitches(&p), [60, 72, 60, 48]);
        // Half the depth, half the swing.
        let half = apply(
            &[slot(MidiFx::Modulator {
                shape: LfoShape::Sine,
                period: 3,
                depth: 50,
                target: ModTarget::Pitch,
                seed: 1,
            })],
            notes.clone(),
        );
        assert_eq!(pitches(&half), [60, 66, 60, 54]);
        // A controller does not touch the notes.
        let c = apply(&[fx(ModTarget::Controller { number: 74 })], notes.clone());
        assert_eq!(c, notes);
    }

    #[test]
    fn a_controller_modulator_sends_the_cycle_as_controller_changes() {
        let chain = [slot(MidiFx::Modulator {
            shape: LfoShape::Triangle,
            period: 4, // 4 beats
            depth: 100,
            target: ModTarget::Controller { number: 74 },
            seed: 1,
        })];
        let c = controls(&chain, 0.0, 8.0);
        assert!(c.iter().all(|x| x.number == 74 && x.value <= 127));
        assert!(c.windows(2).all(|w| w[0].beat <= w[1].beat));
        // A triangle starts at the bottom, reaches the top at the middle of the cycle.
        assert_eq!(
            c[0],
            ModControl {
                beat: 0.0,
                number: 74,
                value: 1
            }
        );
        let top = c.iter().find(|x| x.beat == 2.0).unwrap();
        assert_eq!(top.value, 127);
        let max_gap = c
            .windows(2)
            .map(|w| w[1].beat - w[0].beat)
            .fold(0.0, f64::max);
        assert!(
            max_gap <= 0.25 + 1e-9,
            "a change at least every quarter beat: {max_gap}"
        );
        // A value that does not move is not sent again.
        let flat = [slot(MidiFx::Modulator {
            shape: LfoShape::Sine,
            period: 3,
            depth: 0,
            target: ModTarget::Controller { number: 7 },
            seed: 1,
        })];
        assert_eq!(
            controls(&flat, 0.0, 100.0),
            [ModControl {
                beat: 0.0,
                number: 7,
                value: 64
            }]
        );
        // Modulators of other targets, switched off ones and empty spans send nothing.
        let off = FxSlot {
            fx: chain[0].fx.clone(),
            on: false,
        };
        assert!(controls(&[off], 0.0, 8.0).is_empty());
        assert!(controls(&chain, 5.0, 5.0).is_empty());
        assert!(controls(&chain, f64::NAN, 5.0).is_empty());
        let velocity = [slot(MidiFx::Modulator {
            shape: LfoShape::Sine,
            period: 3,
            depth: 50,
            target: ModTarget::Velocity,
            seed: 1,
        })];
        assert!(controls(&velocity, 0.0, 8.0).is_empty());
    }

    #[test]
    fn settings_round_trip_through_numbers_and_wild_ones_are_repaired() {
        for fx in MidiFx::all_defaults() {
            let script = match &fx {
                MidiFx::Scripter { source } => source.clone(),
                _ => String::new(),
            };
            assert_eq!(
                MidiFx::from_parts(fx.name(), &fx.numbers(), &script),
                Some(fx)
            );
        }
        let mut arp = ArpSettings {
            rate: ArpRate::EighthTriplet,
            mode: ArpMode::DownUp,
            octaves: 3,
            gate: 140,
            swing: 30,
            step_velocity: 60,
            latch: true,
            sync: true,
            length: 23,
            seed: 77,
            ..ArpSettings::default()
        };
        arp.steps[0].tie = true;
        arp.steps[5].on = false;
        arp.steps[9].chord = true;
        arp.steps[31].transpose = -7;
        arp.steps[31].vel = 3;
        let fx = MidiFx::Arpeggiator(arp);
        assert_eq!(fx.name(), "step_arpeggiator");
        assert_eq!(fx.numbers().len(), 170);
        assert_eq!(MidiFx::from_numbers(fx.name(), &fx.numbers()), Some(fx));
        // Missing numbers are start values (a file with fewer steps, or none at all).
        assert_eq!(
            MidiFx::from_numbers("step_arpeggiator", &[]),
            Some(MidiFx::Arpeggiator(ArpSettings::default()))
        );
        let MidiFx::Arpeggiator(short) =
            MidiFx::from_numbers("step_arpeggiator", &arp_numbers(&arp)[..20]).unwrap()
        else {
            panic!("an arpeggiator");
        };
        assert!(short.steps[0].tie && short.steps[9] == ArpStep::default());
        let MidiFx::Arpeggiator(wild) =
            MidiFx::from_numbers("step_arpeggiator", &[999.0; 170]).unwrap()
        else {
            panic!("an arpeggiator");
        };
        assert_eq!(wild, wild.sanitized());
        assert_eq!((wild.rate, wild.mode), (ArpRate::Sixteenth, ArpMode::Up));
        assert_eq!((wild.steps[7].vel, wild.steps[7].transpose), (127, 24));
        assert_eq!(MidiFx::from_numbers("polka", &[]), None);
        let m = MidiFx::Modulator {
            shape: LfoShape::Random,
            period: 5,
            depth: 70,
            target: ModTarget::Controller { number: 74 },
            seed: 9,
        };
        assert_eq!(MidiFx::from_numbers("modulator", &m.numbers()), Some(m));
        let wild_mod =
            MidiFx::from_numbers("modulator", &[99.0, 99.0, 999.0, 2.0, 999.0, -5.0]).unwrap();
        assert_eq!(
            wild_mod,
            MidiFx::Modulator {
                shape: LfoShape::Sine,
                period: 7,
                depth: 100,
                target: ModTarget::Controller { number: 127 },
                seed: 0
            }
        );
        // The four numbers of the Arpeggiator before the step pattern.
        let wild = MidiFx::from_numbers("arpeggiator", &[99.0, 99.0, 99.0, 999.0]).unwrap();
        assert_eq!(
            wild,
            MidiFx::Arpeggiator(ArpSettings {
                rate: ArpRate::Sixteenth,
                mode: ArpMode::Up,
                octaves: 4,
                gate: 100,
                step_velocity: 0,
                ..ArpSettings::default()
            })
        );
        assert_eq!(
            MidiFx::from_numbers("arpeggiator", &[4.0, 4.0, 2.0, 40.0]),
            Some(MidiFx::Arpeggiator(ArpSettings {
                rate: ArpRate::EighthTriplet,
                mode: ArpMode::AsPlayed,
                octaves: 2,
                gate: 40,
                step_velocity: 0,
                ..ArpSettings::default()
            }))
        );
        let nan = MidiFx::from_numbers("transposer", &[f32::NAN]).unwrap();
        assert_eq!(nan, MidiFx::Transposer { semitones: 0 });
        assert_eq!(
            MidiFx::from_numbers("velocity", &[]),
            Some(MidiFx::Velocity { add: 0, scale: 100 })
        );
    }

    #[test]
    fn effects_stop_at_the_limit_instead_of_using_all_the_memory() {
        let notes: Vec<NoteEvent> = (0..5_000)
            .map(|i| note(i as f64, i as f64 + 1.0e6, 60))
            .collect();
        let chain = [
            slot(MidiFx::Repeater {
                rate: StepRate::ThirtySecond,
            }),
            slot(MidiFx::Chord {
                kind: ChordKind::Seventh,
            }),
        ];
        let began = std::time::Instant::now();
        let out = apply(&chain, notes);
        assert!(out.len() <= MAX_FX_NOTES);
        assert!(began.elapsed().as_secs() < 20, "{:?}", began.elapsed());
        // Many chords one after another through the arpeggiator.
        let chords: Vec<NoteEvent> = (0..20_000)
            .flat_map(|i| {
                (0..4).map(move |k| note(i as f64 * 200.0, i as f64 * 200.0 + 199.0, 40 + k * 4))
            })
            .collect();
        let out = apply(
            &[slot(MidiFx::Arpeggiator(ArpSettings {
                rate: ArpRate::ThirtySecond,
                mode: ArpMode::Random,
                octaves: 4,
                gate: 100,
                ..ArpSettings::default()
            }))],
            chords,
        );
        assert_eq!(out.len(), MAX_FX_NOTES);
        assert!(began.elapsed().as_secs() < 60, "{:?}", began.elapsed());
    }

    /// Swing delays the off-beat sixteenths and nothing else; the random part is the same
    /// every time for a seed, differs between seeds and stays within its limit.
    #[test]
    fn swing_delays_the_off_beats_and_humanize_stays_in_its_range() {
        let swing_only = MidiFx::Swing {
            swing: 100,
            timing: 0,
            velocity: 0,
            seed: 1,
        };
        let grid: Vec<NoteEvent> = (0..16)
            .map(|k| note(k as f64 * 0.25, k as f64 * 0.25 + 0.2, 60))
            .collect();
        let swung = apply(&[slot(swing_only.clone())], grid.clone());
        for (k, (a, b)) in grid.iter().zip(&swung).enumerate() {
            // A full swing is a thirty-second note (an eighth of a beat) on steps 1, 3, 5...
            let late = if k % 2 == 1 { 0.125 } else { 0.0 };
            assert!(
                (b.start_beat - a.start_beat - late).abs() < 1e-9,
                "step {k}"
            );
            assert!((b.end_beat - a.end_beat - late).abs() < 1e-9 && b.vel == a.vel);
        }
        // A note off the grid is not swung.
        let off = apply(&[slot(swing_only)], vec![note(0.3, 0.5, 60)]);
        assert_eq!(off[0].start_beat, 0.3);
        let human = |seed: u32| {
            let fx = MidiFx::Swing {
                swing: 0,
                timing: 100,
                velocity: 100,
                seed,
            };
            let notes = (0..200)
                .map(|k| note(4.0 + k as f64 * 0.5, 4.2 + k as f64 * 0.5, 60))
                .collect();
            apply(&[slot(fx)], notes)
        };
        let (a, b, c) = (human(7), human(7), human(8));
        assert_eq!(a, b, "the same seed gives the same notes");
        assert_ne!(a, c, "another seed gives others");
        let mut moved = 0;
        for (k, n) in a.iter().enumerate() {
            let offset = n.start_beat - (4.0 + k as f64 * 0.5);
            assert!(offset.abs() <= HUMANIZE_BEATS + 1e-9, "{offset}");
            assert!((1..=127).contains(&n.vel));
            moved += usize::from(offset.abs() > 1e-6);
        }
        assert!(moved > 150, "{moved}");
        let velocities: std::collections::BTreeSet<u8> = a.iter().map(|n| n.vel).collect();
        assert!(velocities.len() > 20, "{velocities:?}");
    }

    /// The rhythms are what the ported algorithm gives (three in eight is x..x.x.., a
    /// turn of the textbook x..x..x.), and a held note plays the hits on the grid.
    #[test]
    fn euclid_plays_the_hits_of_the_rhythm() {
        let text = |p, s, r| -> String {
            euclidean_rhythm(p, s, r)
                .iter()
                .map(|h| if *h { 'x' } else { '.' })
                .collect()
        };
        assert_eq!(text(3, 8, 0), "x..x.x..");
        assert_eq!(text(5, 8, 0), "x.xx.xx.");
        assert_eq!(text(4, 16, 0), "x...x...x...x...");
        assert_eq!(text(3, 8, 1), "..x.x..x");
        assert_eq!(text(3, 8, -1), ".x..x.x.");
        assert_eq!(text(0, 4, 0), "....");
        assert_eq!(text(4, 4, 9), "xxxx");
        assert!(euclidean_rhythm(3, 0, 0).is_empty());
        let fx = || MidiFx::Euclid {
            rate: StepRate::Sixteenth,
            steps: 8,
            pulses: 3,
            rotation: 0,
        };
        let out = apply(&[slot(fx())], vec![note(0.0, 4.0, 60)]);
        let starts: Vec<f64> = out.iter().map(|n| n.start_beat).collect();
        assert_eq!(starts, [0.0, 0.75, 1.25, 2.0, 2.75, 3.25]);
        assert!(out
            .iter()
            .all(|n| n.pitch == 60 && n.end_beat - n.start_beat <= 0.225 + 1e-9));
        // A note that starts between steps waits for the next hit and ends with its key.
        let late = apply(&[slot(fx())], vec![note(0.1, 0.8, 62)]);
        assert_eq!(
            late.iter().map(|n| n.start_beat).collect::<Vec<_>>(),
            [0.75]
        );
        assert!(late[0].end_beat <= 0.8 + 1e-9);
    }
}
