//! Step patterns for the Arpeggiator (MIDI FX): the built-in ones its Presets menu lists, and
//! the random ones its Randomize key makes. A pattern is the steps and their count; the rate,
//! the order of the keys and the other settings stay as they are.
use prowave_engine::{ArpSettings, ArpStep, ARP_STEPS};

/// A built-in pattern.
pub struct ArpPattern {
    pub name: &'static str,
    /// One entry a step: velocity (0: a rest), semitones, and what else the step does.
    steps: &'static [(u8, i8, Kind)],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// One key.
    Note,
    /// One key, held through the next step.
    Tie,
    /// All the held keys.
    Chord,
}

use Kind::{Chord, Note, Tie};

const fn n(vel: u8) -> (u8, i8, Kind) {
    (vel, 0, Note)
}

const REST: (u8, i8, Kind) = (0, 0, Note);

pub const PATTERNS: [ArpPattern; 10] = [
    ArpPattern {
        name: "Straight",
        steps: &[n(100); 16],
    },
    ArpPattern {
        name: "Accents",
        steps: &[
            n(127),
            n(64),
            n(80),
            n(64),
            n(112),
            n(64),
            n(80),
            n(64),
            n(127),
            n(64),
            n(80),
            n(64),
            n(112),
            n(64),
            n(96),
            n(72),
        ],
    },
    ArpPattern {
        name: "Offbeat",
        steps: &[REST, n(110), REST, n(96), REST, n(110), REST, n(96)],
    },
    ArpPattern {
        name: "Gallop",
        steps: &[
            n(124),
            REST,
            n(84),
            n(96),
            n(118),
            REST,
            n(84),
            n(96),
            n(124),
            REST,
            n(84),
            n(96),
            n(118),
            REST,
            n(90),
            n(104),
        ],
    },
    ArpPattern {
        name: "Three Three Two",
        steps: &[
            (124, 0, Tie),
            (124, 0, Tie),
            REST,
            (108, 0, Tie),
            (108, 0, Tie),
            REST,
            n(116),
            REST,
            (124, 0, Tie),
            (124, 0, Tie),
            REST,
            (108, 0, Tie),
            (108, 0, Tie),
            REST,
            n(116),
            n(90),
        ],
    },
    ArpPattern {
        name: "Long Short",
        steps: &[
            (118, 0, Tie),
            REST,
            n(84),
            n(96),
            (112, 0, Tie),
            REST,
            n(84),
            n(100),
        ],
    },
    ArpPattern {
        name: "Octave Jump",
        steps: &[
            (120, 0, Note),
            (84, 12, Note),
            (104, 0, Note),
            (84, 12, Note),
            (116, 0, Note),
            (84, 12, Note),
            (104, 0, Note),
            (96, 24, Note),
        ],
    },
    ArpPattern {
        name: "Fifths",
        steps: &[
            (118, 0, Note),
            (88, 7, Note),
            (104, 12, Note),
            (88, 7, Note),
            (112, 0, Note),
            (88, 7, Note),
            (104, 12, Note),
            (96, 19, Note),
        ],
    },
    ArpPattern {
        name: "Stabs",
        steps: &[
            (124, 0, Chord),
            REST,
            n(72),
            n(80),
            REST,
            REST,
            (112, 0, Chord),
            REST,
            n(72),
            n(80),
            REST,
            REST,
            (124, 0, Chord),
            REST,
            n(80),
            n(92),
        ],
    },
    ArpPattern {
        name: "Ramp",
        steps: &[
            n(40),
            n(46),
            n(52),
            n(58),
            n(64),
            n(70),
            n(76),
            n(82),
            n(88),
            n(94),
            n(100),
            n(106),
            n(112),
            n(118),
            n(124),
            n(127),
        ],
    },
];

impl ArpPattern {
    /// Puts the pattern's steps and length into `arp`.
    pub fn apply(&self, arp: &mut ArpSettings) {
        arp.steps = [ArpStep::default(); ARP_STEPS];
        arp.length = self.steps.len().clamp(1, ARP_STEPS) as u8;
        for (step, &(vel, transpose, kind)) in arp.steps.iter_mut().zip(self.steps) {
            *step = ArpStep {
                on: vel > 0,
                vel: if vel > 0 { vel.min(127) } else { 100 },
                tie: kind == Tie,
                chord: kind == Chord,
                transpose,
            };
        }
    }
}

/// A number from 0 up to 1, the same for the same `seed` and `k`.
fn chance(seed: u32, k: u32) -> f32 {
    let mut x = u64::from(seed) << 32 | u64::from(k);
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((x ^ (x >> 31)) >> 40) as f32 / 16_777_216.0
}

/// New steps for the pattern's length as it is, drawn from `seed`: mostly notes with the
/// beats accented, some rests, and now and then a tie, a jump of an octave or a fifth, or a
/// chord. The first step always plays.
pub fn randomize(arp: &mut ArpSettings, seed: u32) {
    let len = usize::from(arp.length.clamp(1, ARP_STEPS as u8));
    arp.steps = [ArpStep::default(); ARP_STEPS];
    for (i, step) in arp.steps.iter_mut().take(len).enumerate() {
        let r = |k: u32| chance(seed, i as u32 * 8 + k);
        let beat = i % 4 == 0;
        let accent = if beat { 104.0 } else { 62.0 };
        *step = ArpStep {
            on: i == 0 || r(0) < if beat { 0.92 } else { 0.72 },
            vel: (accent + r(1) * (127.0 - accent)).round().clamp(1.0, 127.0) as u8,
            tie: i + 1 < len && r(2) < 0.12,
            chord: r(3) < 0.07,
            transpose: match r(4) {
                x if x < 0.10 => 12,
                x if x < 0.15 => -12,
                x if x < 0.22 => 7,
                _ => 0,
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_pattern_fits_and_plays_something() {
        let mut names = Vec::new();
        for p in &PATTERNS {
            assert!(!p.name.is_empty() && !names.contains(&p.name));
            names.push(p.name);
            assert!((1..=ARP_STEPS).contains(&p.steps.len()), "{}", p.name);
            let mut arp = ArpSettings {
                gate: 33,
                swing: 20,
                ..ArpSettings::default()
            };
            arp.steps[31].transpose = 5;
            p.apply(&mut arp);
            assert_eq!(usize::from(arp.length), p.steps.len());
            assert!(arp.steps.iter().any(|s| s.on), "{}", p.name);
            assert_eq!(arp, arp.sanitized(), "{}", p.name);
            // The steps past the pattern are start values; the other settings are kept.
            assert_eq!(arp.steps[31], ArpStep::default());
            assert_eq!((arp.gate, arp.swing), (33, 20));
            // A tie is never on a rest.
            for (i, s) in arp.steps[..p.steps.len()].iter().enumerate() {
                assert!(!(s.tie && !s.on), "{} step {i}", p.name);
            }
        }
        let mut arp = ArpSettings::default();
        PATTERNS[0].apply(&mut arp);
        assert_eq!(arp, ArpSettings::default(), "Straight is the start pattern");
    }

    #[test]
    fn randomize_gives_the_same_pattern_for_the_same_seed_within_the_length() {
        let mut a = ArpSettings {
            length: 12,
            ..ArpSettings::default()
        };
        let mut b = a;
        randomize(&mut a, 7);
        randomize(&mut b, 7);
        assert_eq!(a, b);
        assert_eq!(a, a.sanitized());
        assert!(a.steps[0].on && a.length == 12);
        assert!(a.steps[12..].iter().all(|s| *s == ArpStep::default()));
        assert!(!a.steps[11].tie, "no tie out of the last step");
        randomize(&mut b, 8);
        assert_ne!(a, b);
        // Over many seeds every kind of step turns up, and most steps play.
        let (mut rests, mut ties, mut chords, mut jumps, mut total) = (0, 0, 0, 0, 0);
        for seed in 0..200 {
            let mut arp = ArpSettings {
                length: 32,
                ..ArpSettings::default()
            };
            randomize(&mut arp, seed);
            for s in &arp.steps {
                total += 1;
                rests += usize::from(!s.on);
                ties += usize::from(s.tie);
                chords += usize::from(s.chord);
                jumps += usize::from(s.transpose != 0);
            }
        }
        assert!(
            rests > total / 10 && rests < total / 2,
            "{rests} of {total}"
        );
        assert!(ties > 0 && chords > 0 && jumps > 0);
    }
}
