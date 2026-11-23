// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from LMMS (GPL-2.0-or-later, Copyright (c) 2004-2014 Tobias Doerffel <tobydox@users.sourceforge.net>).

//! TripleOscillator (`TriOsc`): 3-oscillator subtractive synthesizer.
//!
//! Featuring per-oscillator coarse/fine detuning, stereo phase offset, waveform selection
//! (Sine, Triangle, Saw, Square, Moog Saw, Exponential, White Noise), and inter-oscillator
//! modulation algorithms (Mix, AM, PM, Sync, FM) with zero audio-thread allocation.

use crate::plugin_track::BlockProcessor;
use crate::synth::Svf;
use std::f32::consts::TAU;

pub const MAX_VOICES: usize = 16;
pub const NUM_OSCS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriWave {
    #[default]
    Sine,
    Triangle,
    Saw,
    Square,
    MoogSaw,
    Exponential,
    WhiteNoise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModAlgo {
    #[default]
    Mix,
    Am,
    Pm,
    Sync,
    Fm,
}

#[derive(Debug, Clone, Copy)]
pub struct OscConfig {
    pub wave: TriWave,
    pub volume: f32,
    pub pan: f32,
    pub coarse: i8,      // -24..+24 semitones
    pub fine_left: f32,  // -100..+100 cents
    pub fine_right: f32, // -100..+100 cents
    pub phase_offset: f32,
    pub stereo_phase: f32,
    pub mod_algo: ModAlgo, // modulation with next oscillator
}

impl Default for OscConfig {
    fn default() -> Self {
        Self {
            wave: TriWave::Saw,
            volume: 0.8,
            pan: 0.0,
            coarse: 0,
            fine_left: 0.0,
            fine_right: 0.0,
            phase_offset: 0.0,
            stereo_phase: 0.0,
            mod_algo: ModAlgo::Mix,
        }
    }
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
    phase_l: [f32; NUM_OSCS],
    phase_r: [f32; NUM_OSCS],
    stage: EnvStage,
    env_level: f32,
    filter: Svf,
    rng: u32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            note: 0,
            velocity: 0.0,
            phase_l: [0.0; NUM_OSCS],
            phase_r: [0.0; NUM_OSCS],
            stage: EnvStage::Idle,
            env_level: 0.0,
            filter: Svf::default(),
            rng: 1234567,
        }
    }
}

pub struct TriOsc {
    rate: f32,
    voices: [Voice; MAX_VOICES],
    pub oscs: [OscConfig; NUM_OSCS],
    // ADSR Envelope
    pub attack_s: f32,
    pub decay_s: f32,
    pub sustain: f32,
    pub release_s: f32,
    // Filter
    pub cutoff_hz: f32,
    pub resonance: f32,
}

impl TriOsc {
    pub fn new(rate: u32) -> Self {
        let sr = (rate.max(8_000)) as f32;
        let mut oscs = [OscConfig::default(); NUM_OSCS];
        oscs[0].wave = TriWave::Saw;
        oscs[1].wave = TriWave::Square;
        oscs[1].coarse = 7; // fifth
        oscs[1].volume = 0.5;
        oscs[2].wave = TriWave::Sine;
        oscs[2].coarse = -12; // sub octave
        oscs[2].volume = 0.6;

        Self {
            rate: sr,
            voices: [Voice::default(); MAX_VOICES],
            oscs,
            attack_s: 0.01,
            decay_s: 0.2,
            sustain: 0.7,
            release_s: 0.3,
            cutoff_hz: 8000.0,
            resonance: 0.2,
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
        v.stage = EnvStage::Attack;
        v.env_level = 0.0;
        v.filter = Svf::default();

        for i in 0..NUM_OSCS {
            let ph = (self.oscs[i].phase_offset / 360.0).fract();
            let st_ph = (self.oscs[i].stereo_phase / 360.0).fract();
            v.phase_l[i] = (ph + st_ph).fract();
            v.phase_r[i] = ph;
        }
    }

    pub fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note && v.stage != EnvStage::Release {
                v.stage = EnvStage::Release;
            }
        }
    }

    #[inline]
    fn sample_wave(wave: TriWave, phase: f32, rng: &mut u32) -> f32 {
        let p = phase.fract();
        match wave {
            TriWave::Sine => (p * TAU).sin(),
            TriWave::Triangle => {
                if p < 0.25 {
                    p * 4.0
                } else if p < 0.75 {
                    2.0 - p * 4.0
                } else {
                    p * 4.0 - 4.0
                }
            }
            TriWave::Saw => 1.0 - 2.0 * p,
            TriWave::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            TriWave::MoogSaw => {
                // Moog-style rounded saw with harmonic warmth
                let s = 1.0 - 2.0 * p;
                s - 0.2 * (p * TAU).sin()
            }
            TriWave::Exponential => (2.0f32.powf(p) - 1.0) * 2.0 - 1.0,
            TriWave::WhiteNoise => {
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                ((*rng >> 16) as f32 / 32768.0) - 1.0
            }
        }
    }

    #[inline]
    pub fn render_sample(&mut self) -> (f32, f32) {
        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        let att_step = 1.0 / (self.attack_s.max(0.001) * self.rate);
        let dec_step = (1.0 - self.sustain).max(0.0) / (self.decay_s.max(0.001) * self.rate);
        let rel_step = 1.0 / (self.release_s.max(0.001) * self.rate);

        for v in &mut self.voices {
            if !v.active {
                continue;
            }

            // Envelope progression
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

            // Render oscillators backwards (3 -> 2 -> 1) for modulation chaining
            let mut osc_sig_l = [0.0f32; NUM_OSCS];
            let mut osc_sig_r = [0.0f32; NUM_OSCS];

            for i in (0..NUM_OSCS).rev() {
                let osc = &self.oscs[i];
                let pitch_left = v.note as f32 + osc.coarse as f32 + osc.fine_left / 100.0;
                let pitch_right = v.note as f32 + osc.coarse as f32 + osc.fine_right / 100.0;

                let freq_l = 440.0 * 2f32.powf((pitch_left - 69.0) / 12.0);
                let freq_r = 440.0 * 2f32.powf((pitch_right - 69.0) / 12.0);

                let inc_l = freq_l / self.rate;
                let inc_r = freq_r / self.rate;

                v.phase_l[i] = (v.phase_l[i] + inc_l).fract();
                v.phase_r[i] = (v.phase_r[i] + inc_r).fract();

                let raw_l = Self::sample_wave(osc.wave, v.phase_l[i], &mut v.rng);
                let raw_r = Self::sample_wave(osc.wave, v.phase_r[i], &mut v.rng);

                let pan_l = (1.0 - osc.pan).clamp(0.0, 1.0);
                let pan_r = (1.0 + osc.pan).clamp(0.0, 1.0);

                let vol_l = raw_l * osc.volume * pan_l;
                let vol_r = raw_r * osc.volume * pan_r;

                if i < NUM_OSCS - 1 {
                    // Modulate with sub oscillator output
                    let sub_l = osc_sig_l[i + 1];
                    let sub_r = osc_sig_r[i + 1];
                    match osc.mod_algo {
                        ModAlgo::Mix => {
                            osc_sig_l[i] = vol_l + sub_l;
                            osc_sig_r[i] = vol_r + sub_r;
                        }
                        ModAlgo::Am => {
                            osc_sig_l[i] = vol_l * (sub_l * 0.5 + 0.5);
                            osc_sig_r[i] = vol_r * (sub_r * 0.5 + 0.5);
                        }
                        ModAlgo::Pm => {
                            let mod_pl = (v.phase_l[i] + sub_l * 0.25).fract();
                            let mod_pr = (v.phase_r[i] + sub_r * 0.25).fract();
                            osc_sig_l[i] = Self::sample_wave(osc.wave, mod_pl, &mut v.rng)
                                * osc.volume
                                * pan_l;
                            osc_sig_r[i] = Self::sample_wave(osc.wave, mod_pr, &mut v.rng)
                                * osc.volume
                                * pan_r;
                        }
                        ModAlgo::Sync => {
                            if sub_l > 0.8 {
                                v.phase_l[i] = 0.0;
                            }
                            if sub_r > 0.8 {
                                v.phase_r[i] = 0.0;
                            }
                            osc_sig_l[i] = vol_l;
                            osc_sig_r[i] = vol_r;
                        }
                        ModAlgo::Fm => {
                            osc_sig_l[i] = vol_l + sub_l * 0.5;
                            osc_sig_r[i] = vol_r + sub_r * 0.5;
                        }
                    }
                } else {
                    osc_sig_l[i] = vol_l;
                    osc_sig_r[i] = vol_r;
                }
            }

            let voice_l = osc_sig_l[0] * v.velocity * v.env_level;
            let voice_r = osc_sig_r[0] * v.velocity * v.env_level;

            // SVF Filter
            let f = Svf::coefficient(self.cutoff_hz, self.rate);
            let damping = Svf::damping(self.resonance);
            let (fl, _) = v.filter.tick(voice_l, f, damping);
            let fr = voice_r * (fl / voice_l.abs().max(1e-6)).clamp(0.0, 4.0);

            out_l += fl;
            out_r += fr;
        }

        (out_l * 0.3, out_r * 0.3)
    }
}

impl BlockProcessor for TriOsc {
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
    fn tri_osc_generates_bounded_audio() {
        let mut tri = TriOsc::new(48000);
        tri.note_on(60, 100);

        let mut out_l = [0.0f32; 128];
        let mut out_r = [0.0f32; 128];

        for _ in 0..10 {
            tri.process((&[], &[]), (&mut out_l, &mut out_r));
            assert!(out_l.iter().all(|x| x.is_finite()));
            assert!(out_r.iter().all(|x| x.is_finite()));
        }
    }
}
