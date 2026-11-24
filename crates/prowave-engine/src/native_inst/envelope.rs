// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! An attack, decay, sustain, release envelope for the native instruments' voices.
//!
//! Logic Pro equivalent: the ADSR envelope of every Logic synthesizer.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Release,
}

/// The envelope's times and level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsrTimes {
    pub attack_s: f32,
    pub decay_s: f32,
    /// 0 to 1.
    pub sustain: f32,
    pub release_s: f32,
}

/// A linear attack to 1, then exponential falls toward the sustain level and, once the key
/// is up, toward silence. Times are those to fall 60 dB (the attack's is its whole length).
#[derive(Clone, Copy, Debug)]
pub struct Adsr {
    stage: Stage,
    level: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new()
    }
}

/// Below this (-80 dB) a released envelope is over.
const FLOOR: f32 = 1e-4;

impl Adsr {
    pub const fn new() -> Self {
        Adsr {
            stage: Stage::Idle,
            level: 0.0,
        }
    }

    /// The key goes down (from the level it has, so a taken voice does not click).
    pub fn gate_on(&mut self) {
        self.stage = Stage::Attack;
    }

    /// The key goes up.
    pub fn gate_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    /// Silent at once.
    pub fn kill(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    pub fn is_idle(&self) -> bool {
        self.stage == Stage::Idle
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    /// The multiplier a fall of 60 dB in `seconds` takes each sample.
    pub fn fall(seconds: f32, rate: f32) -> f32 {
        if seconds <= 0.0 {
            0.0
        } else {
            (-6.908 / (seconds * rate).max(1.0)).exp()
        }
    }

    /// The next sample's level. `attack_step` is 1 / (attack seconds * rate) (1 or more for
    /// none); `decay` and `release` come from [`Adsr::fall`].
    #[inline]
    pub fn tick(&mut self, attack_step: f32, decay: f32, sustain: f32, release: f32) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.level += attack_step;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level = sustain + (self.level - sustain) * decay;
            }
            Stage::Release => {
                self.level *= release;
                if self.level < FLOOR {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level
    }
}

/// [`AdsrTimes`] as the per-sample numbers [`Adsr::tick`] takes, at `rate`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsrRates {
    pub attack_step: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl AdsrRates {
    pub fn new(t: &AdsrTimes, rate: f32) -> Self {
        AdsrRates {
            attack_step: if t.attack_s <= 0.0 {
                1.0
            } else {
                1.0 / (t.attack_s * rate).max(1.0)
            },
            decay: Adsr::fall(t.decay_s, rate),
            sustain: t.sustain.clamp(0.0, 1.0),
            release: Adsr::fall(t.release_s, rate),
        }
    }

    #[inline]
    pub fn tick(&self, env: &mut Adsr) -> f32 {
        env.tick(self.attack_step, self.decay, self.sustain, self.release)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rises_falls_to_the_sustain_level_and_ends_after_the_release() {
        let rate = 48000.0;
        let r = AdsrRates::new(
            &AdsrTimes {
                attack_s: 0.01,
                decay_s: 0.1,
                sustain: 0.5,
                release_s: 0.2,
            },
            rate,
        );
        let mut e = Adsr::new();
        assert!(e.is_idle() && r.tick(&mut e) == 0.0);
        e.gate_on();
        let mut peak_at = 0;
        for i in 0..48000 {
            if r.tick(&mut e) >= 1.0 && peak_at == 0 {
                peak_at = i;
            }
        }
        assert!((470..=490).contains(&peak_at), "{peak_at}");
        assert!((e.level() - 0.5).abs() < 1e-3, "{}", e.level());
        e.gate_off();
        let mut n = 0;
        while !e.is_idle() && n < 96000 {
            r.tick(&mut e);
            n += 1;
        }
        // 60 dB in 0.2 s from 0.5: -80 dB a little later.
        assert!((9000..=13000).contains(&n), "{n}");
        assert_eq!(e.level(), 0.0);
        // No times at all: on at once, off at once.
        let z = AdsrRates::new(
            &AdsrTimes {
                attack_s: 0.0,
                decay_s: 0.0,
                sustain: 1.0,
                release_s: 0.0,
            },
            rate,
        );
        e.gate_on();
        assert_eq!(z.tick(&mut e), 1.0);
        e.gate_off();
        assert_eq!(z.tick(&mut e), 0.0);
        assert!(e.is_idle());
    }
}
