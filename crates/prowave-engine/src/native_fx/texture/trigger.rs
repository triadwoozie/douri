// Copyright 2017 Emilie Gillet. MIT licence (the notice is in mod.rs).
// Rust port: Copyright (C) 2026 Pro Wave contributors, GPL-3.0-or-later.
//
// Ported from Beads' dsp/trigger_generator.cc: the regular (DENSITY below noon) or random
// (above noon) pulse train that starts grains. The clock divider for an external SEED gate
// is not ported: Pro Wave has no seed input.

use super::dsp::{
    constrain, extract_gate_flags, semitones_to_ratio, Random, GATE_HIGH, GATE_RISING,
};

/// `SeedButtonState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedState {
    Off,
    On,
    /// Beads' state at power-on: grains are seeded continuously by DENSITY.
    Locked,
}

/// `TriggerGenerator::Output`.
#[derive(Clone, Copy, Debug, Default)]
pub struct TriggerOutput {
    pub trigger: bool,
    pub gate: bool,
    pub fractional_sample: f32,
    pub regularity: f32,
    pub phase: f32,
}

const MAX_GRAIN_FREQUENCY: f32 = 130.812_78;

pub struct TriggerGenerator {
    phase: f32,
    frequency: f32,
    ratio: f32,
    max_pulse_frequency: f32,
    elapsed_since_last_pulse: i32,
    previous_flags: u8,
}

/// `LogOneMinusX`.
#[inline]
fn log_one_minus_x(x: f32) -> f32 {
    -x * (1.0 + x * (0.5 + x * (0.3333 + x * 0.25)))
}

impl TriggerGenerator {
    pub fn new(sample_rate: f32) -> Self {
        let elapsed = (sample_rate / 2.0) as i32;
        Self {
            phase: 0.0,
            frequency: 1.0 / elapsed as f32,
            ratio: 1.0,
            max_pulse_frequency: MAX_GRAIN_FREQUENCY / sample_rate,
            elapsed_since_last_pulse: elapsed,
            previous_flags: 0,
        }
    }

    fn pulse_train(
        &mut self,
        rng: &mut Random,
        flags: &[u8],
        frequency: f32,
        randomize: bool,
        out: &mut [TriggerOutput],
    ) {
        if !randomize {
            self.ratio = 1.0;
        }
        for (o, flags) in out.iter_mut().zip(flags) {
            let f = constrain(frequency * self.ratio, 0.000_001, 0.25);
            let gate = flags & GATE_HIGH != 0;
            let trigger = flags & GATE_RISING != 0;
            self.phase += if gate { f } else { 0.0 };
            let mut t = 0.0;
            let mut reset = false;
            if trigger {
                self.phase = 0.0;
                reset = true;
            } else if self.phase >= 1.0 {
                self.phase -= 1.0;
                t = self.phase / f;
                reset = true;
            }
            if reset && randomize {
                self.ratio = -1.0 / log_one_minus_x(0.1 + 0.9 * rng.float());
            }
            o.fractional_sample = t;
            o.trigger = reset && gate;
            o.gate = self.phase < 0.5 && gate;
            o.regularity = if randomize { 0.0 } else { 1.0 };
            o.phase = self.phase;
        }
    }

    /// `Render` with no external SEED input. `density` and `exponential_fm` as in Beads.
    pub fn render(
        &mut self,
        rng: &mut Random,
        seed: SeedState,
        density: f32,
        exponential_fm: f32,
        out: &mut [TriggerOutput],
    ) {
        let density = constrain(density, 0.0, 1.0);
        let fm = semitones_to_ratio(exponential_fm * 60.0);
        let mut merged = [0u8; super::processor::MAX_BLOCK];
        let n = out.len().min(merged.len());
        for m in merged.iter_mut().take(n) {
            let gate = seed != SeedState::Off;
            self.previous_flags = extract_gate_flags(self.previous_flags, gate);
            *m = self.previous_flags;
        }
        let out = &mut out[..n];
        if density < 0.48 {
            let f = self.max_pulse_frequency * semitones_to_ratio(-density * 192.0) * fm;
            self.pulse_train(rng, &merged[..n], f, false, out);
        } else if density > 0.52 {
            let f =
                0.5 * self.max_pulse_frequency * semitones_to_ratio(-(1.0 - density) * 192.0) * fm;
            self.pulse_train(rng, &merged[..n], f, true, out);
        } else if seed == SeedState::Locked {
            let o = TriggerOutput {
                trigger: false,
                gate: false,
                fractional_sample: 0.0,
                regularity: 0.0,
                phase: self.phase,
            };
            out.fill(o);
        } else {
            // Manual taps.
            for (o, flags) in out.iter_mut().zip(&merged[..n]) {
                self.phase += self.frequency;
                let tap = flags & GATE_RISING != 0;
                if tap {
                    self.phase = 0.0;
                    self.frequency = 1.0 / self.elapsed_since_last_pulse as f32;
                    self.elapsed_since_last_pulse = 0;
                }
                self.elapsed_since_last_pulse = self.elapsed_since_last_pulse.saturating_add(1);
                if self.phase >= 1.0 {
                    self.phase = 1.0;
                }
                o.fractional_sample = 0.0;
                o.trigger = tap;
                o.gate = flags & GATE_HIGH != 0;
                o.regularity = 1.0 - (density - 0.48) * 25.0;
                o.phase = self.phase;
            }
        }
    }
}
