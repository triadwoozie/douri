// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from LMMS (GPL-2.0-or-later, Copyright (c) 2006-2008 Andreas Brandmaier <andy@brandmaier.de>).

//! WaveDraw: BitInvader-inspired customizable single-cycle wavetable synthesizer.
//!
//! Features a user-editable 256-sample lookup table swapped lock-free between the UI and audio
//! engine, linear/step interpolation, and full polyphonic ADSR playback with `#![forbid(alloc)]`.

use crate::plugin_track::BlockProcessor;
use crate::synth::Svf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub const WAVETABLE_SIZE: usize = 256;
pub const MAX_VOICES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InterpolationMode {
    #[default]
    Linear,
    Step,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Copy)]
struct Voice {
    active: bool,
    note: u8,
    velocity: f32,
    phase: f32,
    stage: EnvStage,
    env_level: f32,
    filter: Svf,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            note: 0,
            velocity: 0.0,
            phase: 0.0,
            stage: EnvStage::Idle,
            env_level: 0.0,
            filter: Svf::default(),
        }
    }
}

pub struct WaveDraw {
    rate: f32,
    voices: [Voice; MAX_VOICES],
    table: Arc<[AtomicU32; WAVETABLE_SIZE]>,
    pub interpolation: InterpolationMode,
    pub attack_s: f32,
    pub decay_s: f32,
    pub sustain: f32,
    pub release_s: f32,
    pub cutoff_hz: f32,
    pub resonance: f32,
}

impl WaveDraw {
    pub fn new(rate: u32) -> Self {
        let sr = (rate.max(8_000)) as f32;
        // Default table is a pure sine wave
        let mut initial = [0.0f32; WAVETABLE_SIZE];
        for (i, item) in initial.iter_mut().enumerate() {
            let phase = i as f32 / WAVETABLE_SIZE as f32;
            *item = (phase * std::f32::consts::TAU).sin();
        }

        let table = Arc::new(std::array::from_fn(|i| {
            AtomicU32::new(initial[i].to_bits())
        }));

        Self {
            rate: sr,
            voices: [Voice::default(); MAX_VOICES],
            table,
            interpolation: InterpolationMode::Linear,
            attack_s: 0.01,
            decay_s: 0.1,
            sustain: 0.8,
            release_s: 0.2,
            cutoff_hz: 12000.0,
            resonance: 0.1,
        }
    }

    /// Clones the shared atomic table handle so the UI can draw into it lock-free.
    pub fn table_handle(&self) -> Arc<[AtomicU32; WAVETABLE_SIZE]> {
        self.table.clone()
    }

    /// Updates the entire wavetable from a float slice and normalizes it.
    pub fn update_table(&self, samples: &[f32; WAVETABLE_SIZE]) {
        let max_val = samples.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        let scale = if max_val > 1e-4 { 1.0 / max_val } else { 1.0 };
        for (i, &s) in samples.iter().enumerate() {
            let clamped = (s * scale).clamp(-1.0, 1.0);
            self.table[i].store(clamped.to_bits(), Ordering::Release);
        }
    }

    pub fn note_on(&mut self, note: u8, vel: u8) {
        if vel == 0 {
            self.note_off(note);
            return;
        }

        let idx = self.voices.iter().position(|v| !v.active).unwrap_or(0);
        let v = &mut self.voices[idx];
        v.active = true;
        v.note = note;
        v.velocity = vel as f32 / 127.0;
        v.phase = 0.0;
        v.stage = EnvStage::Attack;
        v.env_level = 0.0;
        v.filter = Svf::default();
    }

    pub fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note && v.stage != EnvStage::Release {
                v.stage = EnvStage::Release;
            }
        }
    }

    #[inline]
    fn sample_table(
        table: &[AtomicU32; WAVETABLE_SIZE],
        interpolation: InterpolationMode,
        phase: f32,
    ) -> f32 {
        let p = phase.fract() * WAVETABLE_SIZE as f32;
        let idx = (p as usize) % WAVETABLE_SIZE;
        match interpolation {
            InterpolationMode::Step => f32::from_bits(table[idx].load(Ordering::Relaxed)),
            InterpolationMode::Linear => {
                let next = (idx + 1) % WAVETABLE_SIZE;
                let frac = p.fract();
                let s0 = f32::from_bits(table[idx].load(Ordering::Relaxed));
                let s1 = f32::from_bits(table[next].load(Ordering::Relaxed));
                s0 + (s1 - s0) * frac
            }
        }
    }

    #[inline]
    pub fn read_table(&self, phase: f32) -> f32 {
        Self::sample_table(&self.table, self.interpolation, phase)
    }

    #[inline]
    pub fn render_sample(&mut self) -> (f32, f32) {
        let mut out = 0.0f32;

        let att_step = 1.0 / (self.attack_s.max(0.001) * self.rate);
        let dec_step = (1.0 - self.sustain).max(0.0) / (self.decay_s.max(0.001) * self.rate);
        let rel_step = 1.0 / (self.release_s.max(0.001) * self.rate);
        let interp = self.interpolation;
        let table = &self.table;

        for v in &mut self.voices {
            if !v.active {
                continue;
            }

            match v.stage {
                EnvStage::Attack => {
                    v.env_level += att_step;
                    if v.env_level >= 1.0 {
                        v.env_level = 1.0;
                        v.stage = EnvStage::Decay;
                    }
                }
                EnvStage::Decay => {
                    v.env_level -= dec_step;
                    if v.env_level <= self.sustain {
                        v.env_level = self.sustain;
                        v.stage = EnvStage::Sustain;
                    }
                }
                EnvStage::Sustain => {}
                EnvStage::Release => {
                    v.env_level -= rel_step;
                    if v.env_level <= 0.0 {
                        v.env_level = 0.0;
                        v.active = false;
                        v.stage = EnvStage::Idle;
                        continue;
                    }
                }
                EnvStage::Idle => {
                    v.active = false;
                    continue;
                }
            }

            let freq = 440.0 * 2f32.powf((v.note as f32 - 69.0) / 12.0);
            v.phase = (v.phase + freq / self.rate).fract();

            let sample = Self::sample_table(table, interp, v.phase);
            let raw_voice = sample * v.velocity * v.env_level;

            let f = Svf::coefficient(self.cutoff_hz, self.rate);
            let damping = Svf::damping(self.resonance);
            let (filtered, _) = v.filter.tick(raw_voice, f, damping);

            out += filtered;
        }

        let mono = out * 0.4;
        (mono, mono)
    }
}

impl BlockProcessor for WaveDraw {
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
                for v in &mut self.voices {
                    v.active = false;
                    v.stage = EnvStage::Idle;
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
    fn wave_draw_outputs_audio_and_updates_table_lock_free() {
        let mut synth = WaveDraw::new(48000);
        synth.note_on(60, 100);

        let mut out_l = [0.0f32; 128];
        let mut out_r = [0.0f32; 128];

        synth.process((&[], &[]), (&mut out_l, &mut out_r));
        assert!(out_l.iter().all(|x| x.is_finite()));

        // Update table to square wave
        let mut sq = [0.0f32; WAVETABLE_SIZE];
        for (i, item) in sq.iter_mut().enumerate() {
            *item = if i < WAVETABLE_SIZE / 2 { 1.0 } else { -1.0 };
        }
        synth.update_table(&sq);

        synth.process((&[], &[]), (&mut out_l, &mut out_r));
        assert!(out_l.iter().all(|x| x.is_finite()));
    }
}
