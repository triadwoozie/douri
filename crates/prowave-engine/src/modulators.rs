// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from LMMS (GPL-2.0-or-later, Copyright (c) LMMS developers).

//! Real-time modulation controllers: Peak Follower and LFO Controller.
//!
//! Provides dynamic parameter modulation links (sidechain ducking, auto-pan, tremolo,
//! filter sweeps) with zero audio-thread heap allocations.

use std::f32::consts::TAU;

/// Waveform shapes available for the LFO controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LfoShape {
    #[default]
    Sine,
    Triangle,
    Saw,
    Square,
    SampleAndHold,
}

/// An envelope-following Peak Controller (sidechain detector / ducking modulator).
#[derive(Debug, Clone, Copy)]
pub struct PeakFollower {
    rate: f32,
    attack_ms: f32,
    release_ms: f32,
    amount: f32,
    invert: bool,
    current_level: f32,
    att_coef: f32,
    rel_coef: f32,
}

impl PeakFollower {
    pub fn new(rate: f32) -> Self {
        let mut pf = Self {
            rate: rate.max(8_000.0),
            attack_ms: 10.0,
            release_ms: 100.0,
            amount: 1.0,
            invert: false,
            current_level: 0.0,
            att_coef: 0.0,
            rel_coef: 0.0,
        };
        pf.recalculate();
        pf
    }

    pub fn set_attack_ms(&mut self, ms: f32) {
        self.attack_ms = ms.max(0.1);
        self.recalculate();
    }

    pub fn set_release_ms(&mut self, ms: f32) {
        self.release_ms = ms.max(1.0);
        self.recalculate();
    }

    pub fn set_amount(&mut self, amount: f32) {
        self.amount = amount;
    }

    pub fn set_invert(&mut self, invert: bool) {
        self.invert = invert;
    }

    fn recalculate(&mut self) {
        let att_s = (self.attack_ms * 0.001).max(1e-5);
        let rel_s = (self.release_ms * 0.001).max(1e-4);
        self.att_coef = (-1.0 / (att_s * self.rate)).exp();
        self.rel_coef = (-1.0 / (rel_s * self.rate)).exp();
    }

    /// Feeds one stereo sample frame and returns the normalized modulation value.
    #[inline]
    pub fn process_frame(&mut self, left: f32, right: f32) -> f32 {
        let peak = left.abs().max(right.abs());
        let coef = if peak > self.current_level {
            self.att_coef
        } else {
            self.rel_coef
        };
        self.current_level = peak + (self.current_level - peak) * coef;

        let mut out = (self.current_level * self.amount).clamp(0.0, 1.0);
        if self.invert {
            out = 1.0 - out;
        }
        out
    }

    /// Resets the envelope tracker to silence.
    pub fn reset(&mut self) {
        self.current_level = 0.0;
    }

    pub fn level(&self) -> f32 {
        self.current_level
    }
}

/// A flexible Low Frequency Oscillator (LFO) for modulating parameters.
#[derive(Debug, Clone, Copy)]
pub struct LfoController {
    rate: f32,
    frequency_hz: f32,
    shape: LfoShape,
    phase: f32,
    depth: f32,
    offset: f32,
    bipolar: bool,
    rng_state: u32,
    sh_val: f32,
}

impl LfoController {
    pub fn new(rate: f32) -> Self {
        Self {
            rate: rate.max(8_000.0),
            frequency_hz: 1.0,
            shape: LfoShape::Sine,
            phase: 0.0,
            depth: 1.0,
            offset: 0.0,
            bipolar: true,
            rng_state: 123456789,
            sh_val: 0.0,
        }
    }

    pub fn set_frequency(&mut self, hz: f32) {
        self.frequency_hz = hz.clamp(0.01, 100.0);
    }

    pub fn set_shape(&mut self, shape: LfoShape) {
        self.shape = shape;
    }

    pub fn set_depth(&mut self, depth: f32) {
        self.depth = depth.clamp(0.0, 1.0);
    }

    pub fn set_offset(&mut self, offset: f32) {
        self.offset = offset;
    }

    pub fn set_bipolar(&mut self, bipolar: bool) {
        self.bipolar = bipolar;
    }

    /// Advances the LFO by one frame and outputs the modulation value.
    #[inline]
    pub fn step(&mut self) -> f32 {
        let phase_inc = self.frequency_hz / self.rate;
        self.phase += phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            // Update sample & hold on cycle wrap
            self.rng_state = self
                .rng_state
                .wrapping_mul(1664525)
                .wrapping_add(1013904223);
            self.sh_val = ((self.rng_state >> 16) as f32 / 32768.0) - 1.0;
        }

        let raw = match self.shape {
            LfoShape::Sine => (self.phase * TAU).sin(),
            LfoShape::Triangle => {
                if self.phase < 0.25 {
                    self.phase * 4.0
                } else if self.phase < 0.75 {
                    2.0 - self.phase * 4.0
                } else {
                    self.phase * 4.0 - 4.0
                }
            }
            LfoShape::Saw => 1.0 - 2.0 * self.phase,
            LfoShape::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleAndHold => self.sh_val,
        };

        if self.bipolar {
            (raw * self.depth + self.offset).clamp(-1.0, 1.0)
        } else {
            let unipolar = (raw * 0.5 + 0.5) * self.depth + self.offset;
            unipolar.clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_follower_tracks_envelope() {
        let mut pf = PeakFollower::new(48000.0);
        pf.set_attack_ms(1.0);
        pf.set_release_ms(10.0);

        // Feed full scale impulse
        for _ in 0..100 {
            let m = pf.process_frame(1.0, 1.0);
            assert!((0.0..=1.0).contains(&m));
        }
        assert!(pf.level() > 0.8);

        // Decay on silence
        for _ in 0..2000 {
            pf.process_frame(0.0, 0.0);
        }
        assert!(pf.level() < 0.05);
    }

    #[test]
    fn lfo_controller_outputs_bounded_waveforms() {
        let mut lfo = LfoController::new(48000.0);
        lfo.set_frequency(5.0);

        for shape in [
            LfoShape::Sine,
            LfoShape::Triangle,
            LfoShape::Saw,
            LfoShape::Square,
            LfoShape::SampleAndHold,
        ] {
            lfo.set_shape(shape);
            for _ in 0..1000 {
                let v = lfo.step();
                assert!(
                    (-1.0..=1.0).contains(&v),
                    "Shape {:?} produced {}",
                    shape,
                    v
                );
            }
        }
    }
}
