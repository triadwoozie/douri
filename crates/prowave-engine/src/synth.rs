//! Built-in synthesizer building blocks: patch description, band-limited oscillators, a
//! state-variable filter and the drum-kit note mapping. Everything here is allocation-free
//! and safe to call from the audio thread.
//!
//! Logic Pro equivalent: the sound-generating part of a Library patch (the instrument slot).
//! One `SynthPatch` per track; the Library maps each patch to one of these.
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Saw,
    Square,
    Triangle,
}

/// Which voice model a patch plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoiceKind {
    /// The original two-oscillator subtractive voice.
    #[default]
    Subtractive,
    /// The additive drawbar organ (Logic's Vintage B3): nine sine drawbars, percussion, key
    /// click and a Leslie rotor.
    DrawbarOrgan,
}

/// The nine drawbar harmonic ratios, in drawbar order: 16', 5⅓', 8', 4', 2⅔', 2', 1⅗', 1⅓',
/// 1'. The 8' drawbar is the note's fundamental.
pub const DRAWBAR_RATIOS: [f32; 9] = [0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];

/// Leslie rotor speed in Hz for a patch's `leslie` value: 1 = chorale, 2 = tremolo.
pub fn leslie_hz(speed: u8) -> f32 {
    match speed {
        1 => 0.8,
        2 => 6.8,
        _ => 0.0,
    }
}

/// Drum-kit mode: the track's notes select a drum (kick, snare, toms, hats) instead of a pitch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kit {
    None,
    Acoustic,
    Electronic,
    Conga,
    FrameDrum,
    Shaker,
    Tambourine,
}

/// Two oscillators + noise → low/high-pass filter with envelope → ADSR amplitude.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SynthPatch {
    pub wave_a: Wave,
    pub wave_b: Wave,
    pub level_a: f32,
    pub level_b: f32,
    /// Oscillator B pitch relative to A, in semitones and extra cents (detune).
    pub semi_b: f32,
    pub detune_b: f32,
    /// Pulse width for `Wave::Square` (0.5 = square).
    pub pulse_width: f32,
    pub noise: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    /// Filter cutoff in Hz; 20 kHz or more bypasses a low-pass.
    pub cutoff: f32,
    /// 0..1 (Q from about 0.7 to 10).
    pub resonance: f32,
    /// How far the amplitude envelope opens the cutoff (0..1.5, in octaves/4).
    pub filter_env: f32,
    pub highpass: bool,
    pub vibrato_hz: f32,
    pub vibrato_cents: f32,
    /// Starting pitch offset in semitones that falls to 0 (kicks, toms).
    pub pitch_drop: f32,
    pub pitch_decay: f32,
    pub gain: f32,
    pub kit: Kit,
    /// The voice model this patch plays.
    pub voice: VoiceKind,
    /// Drawbar levels 0..8, in [`DRAWBAR_RATIOS`] order (drawbar organ only).
    pub drawbars: [u8; 9],
    /// Percussion harmonic: 0 off, 2 (second) or 3 (third) (drawbar organ only).
    pub percussion: u8,
    /// Percussion decay in seconds (drawbar organ only).
    pub percussion_decay: f32,
    /// Percussion level 0..1 (drawbar organ only).
    pub percussion_level: f32,
    /// Key-click level 0..1 (drawbar organ only).
    pub key_click: f32,
    /// Leslie speed: 0 off, 1 chorale, 2 tremolo (drawbar organ only).
    pub leslie: u8,
    /// Leslie depth 0..1 (amplitude and pitch modulation; drawbar organ only).
    pub leslie_depth: f32,
}

impl Default for SynthPatch {
    /// The engine's original sound: a sine with a quiet octave-up partial.
    fn default() -> Self {
        Self {
            wave_a: Wave::Sine,
            wave_b: Wave::Sine,
            level_a: 1.0,
            level_b: 0.25,
            semi_b: 12.0,
            detune_b: 0.0,
            pulse_width: 0.5,
            noise: 0.0,
            attack: 0.004,
            decay: 0.0,
            sustain: 1.0,
            release: 0.09,
            cutoff: 20_000.0,
            resonance: 0.0,
            filter_env: 0.0,
            highpass: false,
            vibrato_hz: 0.0,
            vibrato_cents: 0.0,
            pitch_drop: 0.0,
            pitch_decay: 0.05,
            gain: 1.0,
            kit: Kit::None,
            voice: VoiceKind::Subtractive,
            drawbars: [0, 0, 8, 0, 0, 0, 0, 0, 0],
            percussion: 0,
            percussion_decay: 0.4,
            percussion_level: 0.5,
            key_click: 0.0,
            leslie: 0,
            leslie_depth: 0.0,
        }
    }
}

impl SynthPatch {
    /// Bounds user/project parameters before they enter the real-time voice state.
    pub(crate) fn sanitized(self) -> Self {
        let d = Self::default();
        let finite = |value: f32, fallback: f32, min: f32, max: f32| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        Self {
            level_a: finite(self.level_a, d.level_a, 0.0, 4.0),
            level_b: finite(self.level_b, d.level_b, 0.0, 4.0),
            semi_b: finite(self.semi_b, d.semi_b, -48.0, 48.0),
            detune_b: finite(self.detune_b, d.detune_b, -100.0, 100.0),
            pulse_width: finite(self.pulse_width, d.pulse_width, 0.05, 0.95),
            noise: finite(self.noise, d.noise, 0.0, 1.0),
            attack: finite(self.attack, d.attack, 0.001, 60.0),
            decay: finite(self.decay, d.decay, 0.0, 60.0),
            sustain: finite(self.sustain, d.sustain, 0.0, 1.0),
            release: finite(self.release, d.release, 0.002, 60.0),
            cutoff: finite(self.cutoff, d.cutoff, 20.0, 20_000.0),
            resonance: finite(self.resonance, d.resonance, 0.0, 1.0),
            filter_env: finite(self.filter_env, d.filter_env, 0.0, 1.5),
            vibrato_hz: finite(self.vibrato_hz, d.vibrato_hz, 0.0, 20.0),
            vibrato_cents: finite(self.vibrato_cents, d.vibrato_cents, 0.0, 100.0),
            pitch_drop: finite(self.pitch_drop, d.pitch_drop, -48.0, 48.0),
            pitch_decay: finite(self.pitch_decay, d.pitch_decay, 0.001, 60.0),
            gain: finite(self.gain, d.gain, 0.0, 4.0),
            voice: self.voice,
            drawbars: self.drawbars.map(|level| level.min(8)),
            percussion: if matches!(self.percussion, 2 | 3) {
                self.percussion
            } else {
                0
            },
            percussion_decay: finite(self.percussion_decay, d.percussion_decay, 0.02, 10.0),
            percussion_level: finite(self.percussion_level, d.percussion_level, 0.0, 1.0),
            key_click: finite(self.key_click, d.key_click, 0.0, 1.0),
            leslie: self.leslie.min(2),
            leslie_depth: finite(self.leslie_depth, d.leslie_depth, 0.0, 1.0),
            ..self
        }
    }
}

fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// One oscillator sample; `phase` in 0..1, `dt` = frequency / sample rate.
pub fn osc(wave: Wave, phase: f32, dt: f32, pulse_width: f32) -> f32 {
    match wave {
        Wave::Sine => (phase * TAU).sin(),
        Wave::Saw => 2.0 * phase - 1.0 - poly_blep(phase, dt),
        Wave::Square => {
            let pw = pulse_width.clamp(0.05, 0.95);
            let mut v = if phase < pw { 1.0 } else { -1.0 };
            v += poly_blep(phase, dt);
            // Falling edge phase, wrapped into 0..1; `phase + 1 - pw` lies in (0, 2), so one
            // subtraction does what `% 1.0` (a slow `fmodf` call) did.
            let fall = phase + 1.0 - pw;
            v -= poly_blep(if fall >= 1.0 { fall - 1.0 } else { fall }, dt);
            v
        }
        Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
    }
}

/// Chamberlin state-variable filter.
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    /// Frequency coefficient for `cutoff`; costly (a `sin`), so compute it at control rate.
    pub fn coefficient(cutoff: f32, sample_rate: f32) -> f32 {
        let sample_rate = sample_rate.max(1.0);
        let max_cutoff = (sample_rate * 0.18).max(20.0);
        let cutoff = if cutoff.is_finite() {
            cutoff.clamp(20.0, max_cutoff)
        } else {
            20_000.0f32.min(max_cutoff)
        };
        (2.0 * (PI * cutoff / sample_rate).sin()).min(1.2)
    }

    /// Damping for `resonance` (0..1).
    pub fn damping(resonance: f32) -> f32 {
        1.0 / (0.7 + resonance.clamp(0.0, 1.0) * 9.0)
    }

    /// Largest [`Svf::coefficient`] that keeps the filter stable with `damping`. The Chamberlin
    /// filter diverges once f² + 2·f·damping reaches 4, which a high cutoff with little
    /// resonance (much damping) can reach below the cutoff clamp.
    pub fn max_coefficient(damping: f32) -> f32 {
        0.98 * ((damping * damping + 4.0).sqrt() - damping)
    }

    /// One sample with precomputed [`Svf::coefficient`] and [`Svf::damping`].
    pub fn tick(&mut self, x: f32, f: f32, damping: f32) -> (f32, f32) {
        self.low += f * self.band;
        let high = x - self.low - damping * self.band;
        self.band += f * high;
        (self.low, high)
    }
}

/// Turns a note on a drum-kit track into a drum voice: its own patch and frequency.
///
/// General MIDI drum layout (Kick 35/36, Snare 38/40, Clap 37/39, Closed/Pedal Hat 42/44,
/// Open Hat 46, Toms 41/43/45/47/48/50, Crash 49/57, Ride 51/59) synthesized with DSP
/// modeling inspired by Soundcraft (storytold/soundcraft, dual MIT / Apache-2.0).
fn percussion_voice(kit: Kit, mut p: SynthPatch, pitch: u8) -> (SynthPatch, f32) {
    let shift = ((pitch as f32) - 48.0).clamp(-12.0, 24.0) * 0.015;
    let hz = match kit {
        Kit::Conga => {
            p.wave_a = Wave::Sine;
            p.wave_b = Wave::Triangle;
            p.level_a = 0.85;
            p.level_b = 0.25;
            p.semi_b = 7.0;
            p.resonance = 0.32;
            p.pitch_drop = 6.5;
            p.pitch_decay = 0.035;
            p.gain *= 1.35;
            match pitch % 4 {
                0 => {
                    p.cutoff = 1400.0;
                    p.noise = 0.06;
                    p.decay = 0.34;
                    p.release = 0.08;
                    148.0 * (1.0 + shift)
                }
                1 => {
                    p.cutoff = 1750.0;
                    p.noise = 0.08;
                    p.decay = 0.28;
                    p.release = 0.07;
                    196.0 * (1.0 + shift)
                }
                2 => {
                    p.cutoff = 2600.0;
                    p.noise = 0.35;
                    p.decay = 0.14;
                    p.release = 0.05;
                    262.0 * (1.0 + shift)
                }
                _ => {
                    p.cutoff = 950.0;
                    p.noise = 0.18;
                    p.decay = 0.09;
                    p.release = 0.04;
                    170.0 * (1.0 + shift)
                }
            }
        }
        Kit::FrameDrum => {
            p.wave_a = Wave::Sine;
            p.wave_b = Wave::Sine;
            p.level_a = 0.9;
            p.level_b = 0.3;
            p.semi_b = 12.0;
            p.attack = 0.004;
            p.resonance = 0.25;
            p.pitch_drop = 4.0;
            p.pitch_decay = 0.06;
            p.gain *= 1.45;
            match pitch % 3 {
                0 => {
                    p.cutoff = 900.0;
                    p.noise = 0.05;
                    p.decay = 0.55;
                    p.release = 0.12;
                    92.0 * (1.0 + shift)
                }
                1 => {
                    p.cutoff = 1600.0;
                    p.noise = 0.16;
                    p.decay = 0.30;
                    p.release = 0.08;
                    142.0 * (1.0 + shift)
                }
                _ => {
                    p.cutoff = 2200.0;
                    p.noise = 0.28;
                    p.decay = 0.18;
                    p.release = 0.06;
                    185.0 * (1.0 + shift)
                }
            }
        }
        Kit::Shaker => {
            p.level_a = 0.0;
            p.level_b = 0.0;
            p.highpass = true;
            p.noise = 0.9;
            p.release = 0.03;
            p.gain *= 1.15;
            match pitch % 4 {
                0 => {
                    p.attack = 0.018;
                    p.decay = 0.075;
                    p.cutoff = 6200.0;
                    p.resonance = 0.28;
                }
                1 => {
                    p.attack = 0.004;
                    p.decay = 0.045;
                    p.cutoff = 8500.0;
                    p.resonance = 0.20;
                }
                2 => {
                    p.attack = 0.010;
                    p.decay = 0.11;
                    p.cutoff = 5200.0;
                    p.resonance = 0.38;
                }
                _ => {
                    p.attack = 0.024;
                    p.decay = 0.14;
                    p.cutoff = 7200.0;
                    p.resonance = 0.15;
                }
            }
            440.0
        }
        Kit::Tambourine => {
            p.wave_a = Wave::Triangle;
            p.wave_b = Wave::Sine;
            p.semi_b = 19.0;
            p.highpass = true;
            p.resonance = 0.35;
            p.release = 0.06;
            p.gain *= 1.1;
            match pitch % 3 {
                0 => {
                    p.level_a = 0.25;
                    p.level_b = 0.20;
                    p.noise = 0.85;
                    p.cutoff = 6800.0;
                    p.attack = 0.008;
                    p.decay = 0.22;
                    1180.0
                }
                1 => {
                    p.level_a = 0.45;
                    p.level_b = 0.25;
                    p.noise = 0.90;
                    p.cutoff = 5400.0;
                    p.attack = 0.002;
                    p.decay = 0.28;
                    760.0
                }
                _ => {
                    p.level_a = 0.20;
                    p.level_b = 0.15;
                    p.noise = 0.75;
                    p.cutoff = 8200.0;
                    p.attack = 0.003;
                    p.decay = 0.09;
                    1420.0
                }
            }
        }
        Kit::None | Kit::Acoustic | Kit::Electronic => 440.0,
    };
    (p, hz)
}

pub fn kit_voice(base: &SynthPatch, pitch: u8) -> (SynthPatch, f32) {
    let mut p = SynthPatch {
        kit: base.kit,
        gain: base.gain,
        ..SynthPatch::default()
    };
    p.attack = 0.001;
    p.sustain = 0.0;
    p.level_b = 0.0;
    if matches!(
        base.kit,
        Kit::Conga | Kit::FrameDrum | Kit::Shaker | Kit::Tambourine
    ) {
        return percussion_voice(base.kit, p, pitch);
    }
    let electronic = base.kit == Kit::Electronic;
    let hz = match pitch {
        0..=36 => {
            // Kick (35/36 standard GM)
            p.wave_a = Wave::Sine;
            p.noise = 0.04;
            p.pitch_drop = if electronic { 30.0 } else { 18.0 };
            p.pitch_decay = if electronic { 0.08 } else { 0.045 };
            p.decay = if electronic { 0.38 } else { 0.24 };
            p.release = 0.06;
            p.gain *= 1.6;
            if electronic {
                45.0
            } else {
                52.0
            }
        }
        37 | 39 => {
            // Clap / Side Stick (37/39 GM)
            p.wave_a = Wave::Triangle;
            p.level_a = 0.18;
            p.noise = 0.95;
            p.cutoff = 1150.0;
            p.resonance = 0.4;
            p.decay = if electronic { 0.18 } else { 0.12 };
            p.release = 0.05;
            p.gain *= 1.35;
            350.0
        }
        38 | 40 => {
            // Snare (38/40 GM)
            p.wave_a = if electronic {
                Wave::Square
            } else {
                Wave::Triangle
            };
            p.level_a = 0.55;
            p.noise = 0.85;
            p.pitch_drop = 4.0;
            p.pitch_decay = 0.02;
            p.decay = if electronic { 0.2 } else { 0.15 };
            p.release = 0.05;
            p.cutoff = 9_000.0;
            185.0
        }
        42 | 44 => {
            // Closed Hat / Pedal Hat (42/44 GM)
            p.level_a = 0.0;
            p.noise = 0.7;
            p.highpass = true;
            p.cutoff = 7_000.0;
            p.resonance = 0.15;
            p.decay = if electronic { 0.03 } else { 0.05 };
            p.release = 0.02;
            440.0
        }
        46 => {
            // Open Hat (46 GM)
            p.level_a = 0.0;
            p.noise = 0.75;
            p.highpass = true;
            p.cutoff = 6_500.0;
            p.resonance = 0.1;
            p.decay = 0.35;
            p.release = 0.04;
            440.0
        }
        49 | 57 => {
            // Crash Cymbal (49/57 GM)
            p.level_a = 0.0;
            p.noise = 0.9;
            p.highpass = true;
            p.cutoff = 4_500.0;
            p.resonance = 0.1;
            p.decay = 0.75;
            p.release = 0.1;
            p.gain *= 1.1;
            440.0
        }
        51 | 59 => {
            // Ride Cymbal (51/59 GM)
            p.level_a = 0.15;
            p.wave_a = Wave::Sine;
            p.noise = 0.55;
            p.highpass = true;
            p.cutoff = 5_500.0;
            p.resonance = 0.2;
            p.decay = 0.65;
            p.release = 0.08;
            845.0
        }
        41 | 43 | 45 | 47 | 48 | 50 => {
            // Toms: floor tom (41/43), low tom (45), mid tom (47/48), high tom (50)
            p.wave_a = Wave::Sine;
            p.pitch_drop = if electronic { 14.0 } else { 8.0 };
            p.pitch_decay = 0.09;
            p.decay = 0.3;
            p.release = 0.08;
            p.gain *= 1.2;
            match pitch {
                41 | 43 => 82.0,
                45 => 110.0,
                47 | 48 => 147.0,
                50 => 196.0,
                _ => 120.0,
            }
        }
        _ => {
            // Any other MIDI note: play a crisp percussion tap
            p.wave_a = Wave::Sine;
            p.noise = 0.25;
            p.pitch_drop = 6.0;
            p.pitch_decay = 0.04;
            p.decay = 0.2;
            p.release = 0.05;
            p.gain *= 1.1;
            100.0 + (pitch as f32) * 8.0
        }
    };
    (p, hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sum of |second difference|: grows with high-frequency content.
    fn roughness(x: &[f32]) -> f32 {
        x.windows(3)
            .map(|w| (w[2] - 2.0 * w[1] + w[0]).abs())
            .sum::<f32>()
            / x.len() as f32
    }

    #[test]
    fn oscillators_stay_in_range_and_have_the_right_pitch() {
        let (sr, hz) = (48_000.0f32, 220.0f32);
        for wave in [Wave::Sine, Wave::Saw, Wave::Square, Wave::Triangle] {
            let mut phase = 0.0f32;
            let dt = hz / sr;
            let mut crossings = 0;
            let mut prev = osc(wave, 0.3, dt, 0.5);
            for _ in 0..48_000 {
                phase = (phase + dt).fract();
                let x = osc(wave, phase, dt, 0.5);
                assert!(x.is_finite() && x.abs() <= 1.3, "{wave:?} {x}");
                if (prev < 0.0) != (x < 0.0) {
                    crossings += 1;
                }
                prev = x;
            }
            // 220 Hz for 1 s -> about 440 sign changes.
            assert!((crossings - 440i32).abs() <= 6, "{wave:?}: {crossings}");
        }
    }

    #[test]
    fn lowpass_removes_high_frequency_content() {
        let (sr, hz) = (48_000.0f32, 440.0f32);
        let mut phase = 0.0f32;
        let (mut open, mut dark) = (Vec::new(), Vec::new());
        let mut f = Svf::default();
        for _ in 0..24_000 {
            phase = (phase + hz / sr).fract();
            let x = osc(Wave::Saw, phase, hz / sr, 0.5);
            open.push(x);
            dark.push(f.tick(x, Svf::coefficient(500.0, sr), Svf::damping(0.0)).0);
        }
        assert!(
            roughness(&dark) < roughness(&open) * 0.5,
            "{} vs {}",
            roughness(&dark),
            roughness(&open)
        );
        assert!(dark.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn kit_maps_notes_to_different_drums() {
        let base = SynthPatch {
            kit: Kit::Acoustic,
            ..SynthPatch::default()
        };
        let (kick, kick_hz) = kit_voice(&base, 36);
        let (snare, _) = kit_voice(&base, 38);
        let (hat, _) = kit_voice(&base, 42);
        let (open_hat, _) = kit_voice(&base, 46);
        assert!(kick_hz < 100.0 && kick.pitch_drop > 10.0);
        assert!(snare.noise > 0.5 && snare.level_a > 0.0);
        assert!(hat.highpass && hat.level_a == 0.0);
        assert!(open_hat.decay > hat.decay * 3.0);
        let (e_kick, _) = kit_voice(
            &SynthPatch {
                kit: Kit::Electronic,
                ..SynthPatch::default()
            },
            36,
        );
        assert!(e_kick.decay > kick.decay);
    }

    #[test]
    fn percussion_kits_produce_distinct_voices() {
        let kits = [Kit::Conga, Kit::FrameDrum, Kit::Shaker, Kit::Tambourine];
        let voices: Vec<(SynthPatch, f32)> = kits
            .iter()
            .map(|&kit| {
                kit_voice(
                    &SynthPatch {
                        kit,
                        ..SynthPatch::default()
                    },
                    36,
                )
            })
            .collect();
        // Every pair of percussion kits differs in voice parameters or pitch.
        for i in 0..voices.len() {
            for j in (i + 1)..voices.len() {
                assert_ne!(
                    voices[i], voices[j],
                    "kits {:?} and {:?} sounded identical",
                    kits[i], kits[j]
                );
            }
        }
        // Shaker is pure high-passed noise; Conga & FrameDrum have pitched bodies.
        assert_eq!(voices[2].0.level_a, 0.0);
        assert!(voices[2].0.highpass);
        assert!(voices[0].0.level_a > 0.5);
        assert!(voices[1].1 < voices[0].1);
        assert!(voices[3].0.highpass && voices[3].0.level_a > 0.0);
    }
}
