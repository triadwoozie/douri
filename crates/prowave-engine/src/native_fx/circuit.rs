// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The circuit solver, the oversampler and the circuit
// itself are the vendored crate crates/circuit-dsp (Copyright (c) 2026 Somewhere Systems, MIT,
// https://github.com/somewheresystems/analog-to-vst, template/dsp at commit f5a9d8d), unchanged;
// see its NOTICE.

//! Circuit Overdrive: an op-amp gain stage into a diode pair, solved as a circuit per sample.
//!
//! Logic Pro equivalent: Overdrive and the Pedalboard's drive pedals.
//!
//! The circuit (a netlist: resistors, capacitors, an op-amp, two diodes, a Gain and a Level
//! pot) is solved by `circuit_dsp` at 576 kHz or above, one circuit per channel, between an
//! oversampler's two linear-phase filters; their delay is reported for delay compensation. All
//! memory is allocated in [`CircuitOverdrive::new`]; `process` and `set_param` do not allocate
//! (a moved pot rebuilds the circuit's matrices in place).

use super::{block_effect, stored_values, NativeFx, NativeParam};

pub const ID: &str = "prowave:circuit-overdrive";
pub const NAME: &str = "Circuit Overdrive";

pub const COUNT: usize = 2;
pub const GAIN: usize = 0;
pub const LEVEL: usize = 1;

/// The pots' rotation in percent.
pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::knob("Gain", 0, 100, 50, "%"),
    NativeParam::knob("Level", 0, 100, 50, "%"),
];

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        GAIN | LEVEL => format!("{v}%"),
        _ => String::new(),
    }
}

/// The largest sample the circuit is given (2 V at its input): the model has no supply rails,
/// so a far larger one would only take its solver out of range.
const INPUT_LIMIT: f32 = 10.0;

fn rotation(v: i32) -> f32 {
    v as f32 / 100.0
}

/// The overdrive.
pub struct CircuitOverdrive {
    rate: u32,
    applied: [i32; COUNT],
    circuit: circuit_dsp::Processor,
}

impl CircuitOverdrive {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let applied = stored_values(&PARAMS, values);
        CircuitOverdrive {
            rate,
            applied,
            circuit: circuit_dsp::Processor::new(f64::from(rate), &applied.map(rotation)),
        }
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32]) {
        for v in left.iter_mut().chain(right.iter_mut()) {
            *v = if v.is_finite() {
                v.clamp(-INPUT_LIMIT, INPUT_LIMIT)
            } else {
                0.0
            };
        }
        self.circuit.process(left, right);
        for v in left.iter_mut().chain(right.iter_mut()) {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
    }

    fn latency_samples(&self) -> usize {
        self.circuit.latency()
    }
}

block_effect!(CircuitOverdrive);

impl NativeFx for CircuitOverdrive {
    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = PARAMS.get(index) else { return };
        let v = p.stored(value);
        if self.applied[index] != v {
            self.applied[index] = v;
            self.circuit.set_control(index, rotation(v));
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_signals::{noise, peak, run_blocks, sine};
    use super::*;

    /// The level of `hz` in `x` (a single-bin Fourier sum).
    fn level_at(x: &[f32], hz: f64, rate: f64) -> f64 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, v) in x.iter().enumerate() {
            let a = i as f64 * std::f64::consts::TAU * hz / rate;
            re += f64::from(*v) * a.cos();
            im += f64::from(*v) * a.sin();
        }
        2.0 * (re * re + im * im).sqrt() / x.len() as f64
    }

    #[test]
    fn circuit_does_not_allocate() {
        let mut fx = CircuitOverdrive::new(48000, &[]);
        let (l, r) = (noise(24000, 1.0, 5), noise(24000, 1.0, 11));
        let (a, _) = run_blocks(&mut fx, &l[..12000], &r[..12000], 256);
        // Moving both pots, as automation does, rebuilds the circuit without allocating.
        assert_no_alloc::assert_no_alloc(|| {
            fx.set_param(GAIN, 90.0);
            fx.set_param(LEVEL, 20.0);
        });
        let (b, _) = run_blocks(&mut fx, &l[12000..], &r[12000..], 256);
        assert!(a.iter().chain(&b).all(|x| x.is_finite()));
        assert!(peak(&a) > 0.01 && peak(&b) > 0.001);
    }

    /// A 0.5-amplitude sine comes out audible, finite and with odd harmonics it did not have.
    #[test]
    fn a_sine_gains_harmonics() {
        let rate = 48000.0;
        let x = sine(48000, 0.5, 440.0, rate);
        let mut fx = CircuitOverdrive::new(48000, &[100.0, 100.0]);
        let (y, _) = run_blocks(&mut fx, &x, &x, 512);
        assert!(y.iter().all(|v| v.is_finite()));
        let tail = &y[24000..];
        assert!(peak(tail) > 0.05, "{}", peak(tail));
        let (one, three) = (level_at(tail, 440.0, rate), level_at(tail, 1320.0, rate));
        assert!(level_at(&x[24000..], 1320.0, rate) < 1e-4);
        assert!(
            three > one * 0.05,
            "fundamental {one}, third harmonic {three}"
        );
    }

    /// The oversampling filters' delay is what the effect reports.
    #[test]
    fn the_reported_latency_is_the_delay() {
        let mut fx = CircuitOverdrive::new(48000, &[0.0, 100.0]);
        let lat = fx.latency_samples();
        assert!(lat > 0);
        let mut x = vec![0.0f32; 4000];
        x[1000] = 0.2;
        let (y, _) = run_blocks(&mut fx, &x, &x, 128);
        let at = (0..y.len())
            .max_by(|a, b| y[*a].abs().total_cmp(&y[*b].abs()))
            .unwrap_or(0);
        assert!(at.abs_diff(1000 + lat) <= 2, "peak at {at}, latency {lat}");
    }
}
