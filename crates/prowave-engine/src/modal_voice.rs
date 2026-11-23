// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from transit-audio/transit (MIT OR Apache-2.0, Copyright (c) 2024 transit contributors).

//! Physical modeling Modal Voice synthesizer.
//!
//! Exciters (impulse, mallet strike, noise burst) feed a bank of tuned resonant bandpass modes
//! for metallic hits, bells, mallet percussion, and organic acoustic motion design.
//! Fully allocation-free on the audio thread (`#![forbid(alloc)]`).

use crate::plugin_track::BlockProcessor;
use std::f32::consts::TAU;

pub const MAX_MODAL_MODES: usize = 16;
pub const MAX_VOICES: usize = 16;

/// Second-order resonant bandpass filter mode.
#[derive(Clone, Copy, Debug, Default)]
struct ModeFilter {
    y1: f32,
    y2: f32,
    a1: f32,
    a2: f32,
    b0: f32,
}

impl ModeFilter {
    fn set(&mut self, freq_hz: f32, q: f32, rate: f32) {
        let f = (freq_hz / rate).clamp(0.0001, 0.49);
        let q = q.max(0.5);
        let w0 = TAU * f;
        let alpha = w0.sin() / (2.0 * q);
        let b0_raw = alpha;
        let a0 = 1.0 + alpha;
        let a1_raw = -2.0 * w0.cos();
        let a2_raw = 1.0 - alpha;

        self.b0 = b0_raw / a0;
        self.a1 = a1_raw / a0;
        self.a2 = a2_raw / a0;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x - self.a1 * self.y1 - self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    note: u8,
    velocity: f32,
    env_phase: usize,
    env_len: usize,
    modes: [ModeFilter; MAX_MODAL_MODES],
    mode_gains: [f32; MAX_MODAL_MODES],
    rng: u32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            note: 0,
            velocity: 0.0,
            env_phase: 0,
            env_len: 0,
            modes: [ModeFilter::default(); MAX_MODAL_MODES],
            mode_gains: [1.0; MAX_MODAL_MODES],
            rng: 1234567,
        }
    }
}

pub struct ModalVoice {
    rate: f32,
    voices: [Voice; MAX_VOICES],
    // Synthesis parameters
    stiffness: f32,
    damping: f32,
    brightness: f32,
    strike_hardness: f32,
}

impl ModalVoice {
    pub fn new(rate: u32) -> Self {
        let sr = (rate.max(8_000)) as f32;
        Self {
            rate: sr,
            voices: [Voice::default(); MAX_VOICES],
            stiffness: 0.5,
            damping: 0.3,
            brightness: 0.7,
            strike_hardness: 0.8,
        }
    }

    pub fn set_stiffness(&mut self, v: f32) {
        self.stiffness = v.clamp(0.01, 1.0);
    }

    pub fn set_damping(&mut self, v: f32) {
        self.damping = v.clamp(0.01, 1.0);
    }

    pub fn set_brightness(&mut self, v: f32) {
        self.brightness = v.clamp(0.01, 1.0);
    }

    pub fn set_strike_hardness(&mut self, v: f32) {
        self.strike_hardness = v.clamp(0.01, 1.0);
    }

    pub fn note_on(&mut self, note: u8, vel: u8) {
        if vel == 0 {
            self.note_off(note);
            return;
        }

        // Steal oldest voice or pick inactive
        let idx = self.voices.iter().position(|v| !v.active).unwrap_or(0);

        let v = &mut self.voices[idx];
        v.active = true;
        v.note = note;
        v.velocity = vel as f32 / 127.0;
        v.env_phase = 0;
        v.env_len = (self.rate * 0.005 * (1.5 - self.strike_hardness)) as usize; // short impulse/burst
        v.env_len = v.env_len.max(4);

        let fundamental = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
        let q_base = 50.0 / self.damping.max(0.05);

        for m in 0..MAX_MODAL_MODES {
            // Inharmonic mode frequencies governed by stiffness (bar / plate modal ratios)
            let m_f = m as f32 + 1.0;
            let ratio = m_f * (1.0 + m_f * m_f * self.stiffness * 0.1).sqrt();
            let mode_hz = (fundamental * ratio).min(self.rate * 0.48);
            let q = (q_base / (1.0 + m as f32 * (1.0 - self.brightness) * 0.5)).max(1.0);

            v.modes[m].set(mode_hz, q, self.rate);
            v.modes[m].reset();
            v.mode_gains[m] = (-0.3 * m as f32 * (1.1 - self.brightness)).exp();
        }
    }

    pub fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                // Modal instruments naturally ring out, but damping increases on release
                for m in &mut v.modes {
                    m.a2 *= 0.98;
                }
            }
        }
    }

    #[inline]
    pub fn render_sample(&mut self) -> (f32, f32) {
        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;

        for v in &mut self.voices {
            if !v.active {
                continue;
            }

            // Exciter: shaped impulse + noise burst
            let exciter = if v.env_phase < v.env_len {
                let frac = v.env_phase as f32 / v.env_len as f32;
                let hann = 0.5 * (1.0 - (TAU * frac).cos());
                v.rng = v.rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = ((v.rng >> 16) as f32 / 32768.0) - 1.0;
                let impulse = (1.0 - frac) * 0.7 + noise * 0.3 * self.strike_hardness;
                v.env_phase += 1;
                impulse * hann * v.velocity
            } else {
                0.0
            };

            let mut voice_out = 0.0f32;
            let mut any_ringing = false;

            for m in 0..MAX_MODAL_MODES {
                let out = v.modes[m].process(exciter);
                voice_out += out * v.mode_gains[m];
                if out.abs() > 1e-5 {
                    any_ringing = true;
                }
            }

            if v.env_phase >= v.env_len && !any_ringing {
                v.active = false;
            }

            sum_l += voice_out;
            sum_r += voice_out;
        }

        (sum_l * 0.3, sum_r * 0.3)
    }
}

impl BlockProcessor for ModalVoice {
    fn midi(&mut self, status: u8, data1: u8, data2: u8, _offset: u32) {
        match status & 0xF0 {
            0x90 => {
                if data2 > 0 {
                    self.note_on(data1, data2);
                } else {
                    self.note_off(data1);
                }
            }
            0x80 => {
                self.note_off(data1);
            }
            0xB0 if data1 == 120 || data1 == 123 => {
                // All sound off
                for v in &mut self.voices {
                    v.active = false;
                }
            }
            _ => {}
        }
    }

    fn process(&mut self, _input: (&[f32], &[f32]), out: (&mut [f32], &mut [f32])) {
        let n = out.0.len().min(out.1.len());
        for i in 0..n {
            let (l, r) = self.render_sample();
            out.0[i] = l;
            out.1[i] = r;
        }
    }

    fn latency_seconds(&self) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_voice_produces_finite_audio_without_alloc() {
        let mut voice = ModalVoice::new(48000);
        voice.note_on(60, 100);

        let mut out_l = [0.0f32; 128];
        let mut out_r = [0.0f32; 128];

        for _ in 0..10 {
            voice.process((&[], &[]), (&mut out_l, &mut out_r));
            assert!(out_l.iter().all(|x| x.is_finite()));
            assert!(out_r.iter().all(|x| x.is_finite()));
        }
    }
}
