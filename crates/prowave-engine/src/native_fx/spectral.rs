// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The phase vocoder is the vendored crate
// crates/mi-dsp (MIT, Copyright 2014 Emilie Gillet, https://github.com/pichenettes/eurorack,
// clouds/dsp at commit f4cfa76), its C++ unchanged; see its NOTICE.

//! Pro Wave Spectral: a phase vocoder that records the input as spectra and plays them back
//! shifted, warped, quantized or frozen.
//!
//! Logic Pro equivalent: none directly; nearest are Spectral Gate and the freeze of
//! ChromaVerb.
//!
//! The spectral mode of the texture processor in `mi_dsp::Spectral` (Pro Wave Texture is its
//! successor's granular engine; this mode has no counterpart there). It runs in stereo at
//! 32 kHz, 16 bits, in blocks of 32 frames, behind a converter ([`FixedRateFx`]) whose fixed
//! delay is reported. The mix, the feedback and the reverb are the processor's own. As on the
//! module: Position picks which of the stored spectra plays, Size warps the frequency axis,
//! Density sets how often the stored spectrum is refreshed (and, at its ends, how far the
//! phases are randomized), Texture quantizes the magnitudes below its centre (at the bottom
//! only the strongest partials are left) and sharpens their peaks above it.

use super::{copy_in, stored_values, NativeFx, NativeParam};
use crate::native_inst::rate::FixedRateFx;
use crate::plugin_track::BlockProcessor;
use mi_dsp::{Spectral as Dsp, SpectralBlock, SPECTRAL_BLOCK, SPECTRAL_RATE};

pub const ID: &str = "prowave:spectral";
pub const NAME: &str = "Dōri Spectral";

pub const POSITION: usize = 0;
pub const SIZE: usize = 1;
pub const PITCH: usize = 2;
pub const DENSITY: usize = 3;
pub const TEXTURE: usize = 4;
pub const FEEDBACK: usize = 5;
pub const REVERB: usize = 6;
pub const MIX: usize = 7;
pub const FREEZE: usize = 8;
pub const COUNT: usize = 9;

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::knob("Position", 0, 100, 0, "%"),
    NativeParam::knob("Size", 0, 100, 50, "%"),
    NativeParam::knob("Pitch", -24, 24, 0, "st"),
    NativeParam::knob("Density", 0, 100, 50, "%"),
    NativeParam::knob("Texture", 0, 100, 50, "%"),
    NativeParam::knob("Feedback", 0, 100, 0, "%"),
    NativeParam::knob("Reverb", 0, 100, 0, "%"),
    NativeParam::knob("Mix", 0, 100, 100, "%"),
    NativeParam::choice("Freeze", 0, 0, &["Off", "On"]),
];

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        PITCH => format!("{v}st"),
        FREEZE => if v != 0 { "On" } else { "Off" }.to_string(),
        _ if k < COUNT => format!("{v}%"),
        _ => String::new(),
    }
}

/// The effect.
pub struct Spectral {
    rate: u32,
    applied: [i32; COUNT],
    /// `None` when memory ran out: the effect passes its input on.
    dsp: Option<Dsp>,
    fixed: FixedRateFx,
}

impl Spectral {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        Spectral {
            rate,
            applied: stored_values(&PARAMS, values),
            dsp: Dsp::new(),
            fixed: FixedRateFx::new(SPECTRAL_RATE, rate, SPECTRAL_BLOCK),
        }
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32]) {
        let p = self.applied;
        let Some(dsp) = self.dsp.as_mut() else { return };
        let unit = |k: usize| p[k] as f32 / 100.0;
        let block = SpectralBlock {
            position: unit(POSITION),
            size: unit(SIZE),
            pitch: p[PITCH] as f32,
            density: unit(DENSITY),
            texture: unit(TEXTURE),
            dry_wet: unit(MIX),
            feedback: unit(FEEDBACK),
            reverb: unit(REVERB),
            freeze: p[FREEZE],
        };
        let to_16 = |x: f32| {
            if x.is_finite() {
                (x.clamp(-1.0, 1.0) * 32767.0) as i16
            } else {
                0
            }
        };
        let (mut input, mut output) = ([0i16; SPECTRAL_BLOCK * 2], [0i16; SPECTRAL_BLOCK * 2]);
        self.fixed.process(
            left,
            right,
            |il, ir, ol, or| {
                for (k, (a, b)) in il.iter().zip(ir).enumerate() {
                    input[2 * k] = to_16(*a);
                    input[2 * k + 1] = to_16(*b);
                }
                dsp.process(&block, &input, &mut output);
                for (k, (a, b)) in ol.iter_mut().zip(or.iter_mut()).enumerate() {
                    *a = f32::from(output[2 * k]) / 32768.0;
                    *b = f32::from(output[2 * k + 1]) / 32768.0;
                }
            },
            // The processor mixes the dry signal in itself.
            |_, out| out,
        );
    }
}

impl BlockProcessor for Spectral {
    fn midi(&mut self, _status: u8, _data1: u8, _data2: u8, _offset: u32) {}

    fn process(&mut self, input: (&[f32], &[f32]), out: (&mut [f32], &mut [f32])) {
        let n = copy_in(input, (&mut *out.0, &mut *out.1));
        self.run(&mut out.0[..n], &mut out.1[..n]);
    }

    fn latency_seconds(&self) -> f64 {
        if self.dsp.is_some() {
            self.fixed.latency_frames() as f64 / f64::from(self.rate)
        } else {
            0.0
        }
    }
}

impl NativeFx for Spectral {
    fn set_param(&mut self, index: usize, value: f32) {
        if let Some(p) = PARAMS.get(index) {
            self.applied[index] = p.stored(value);
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_signals::{noise, peak, rms, run_blocks, sine};
    use super::*;

    #[test]
    fn spectral_does_not_allocate() {
        let mut fx = Spectral::new(44100, &[]);
        let (l, r) = (noise(88200, 0.5, 3), noise(88200, 0.5, 4));
        let (a, b) = run_blocks(&mut fx, &l, &r, 256);
        assert!(a.iter().chain(&b).all(|v| v.is_finite()));
        assert!(peak(&a[44100..]) > 0.01, "{}", peak(&a[44100..]));
        assert_no_alloc::assert_no_alloc(|| {
            for (k, p) in PARAMS.iter().enumerate() {
                fx.set_param(k, p.max);
            }
        });
        let (a, _) = run_blocks(&mut fx, &l, &r, 100);
        assert!(a.iter().all(|v| v.is_finite()));
        assert_eq!(fx.fixed.underruns(), 0);
    }

    /// A tone comes through all wet at about its level, an octave up it comes out an octave
    /// higher, and frozen it goes on sounding after the input stops.
    #[test]
    fn a_tone_passes_shifts_and_freezes() {
        let rate = 48000.0;
        let n = 96000;
        let x = sine(n, 0.4, 440.0, rate);
        let crossings = |y: &[f32]| y.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        let mut fx = Spectral::new(48000, &[]);
        let (y, _) = run_blocks(&mut fx, &x, &x, 256);
        let tail = &y[n / 2..];
        assert!(rms(tail) > 0.05, "{}", rms(tail));
        let hz = crossings(tail) as f64 / (tail.len() as f64 / rate);
        assert!((hz / 440.0 - 1.0).abs() < 0.05, "{hz} Hz");

        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[PITCH] = 12.0;
        let mut up = Spectral::new(48000, &values);
        let (y, _) = run_blocks(&mut up, &x, &x, 256);
        let tail = &y[n / 2..];
        let hz = crossings(tail) as f64 / (tail.len() as f64 / rate);
        assert!((hz / 880.0 - 1.0).abs() < 0.08, "{hz} Hz an octave up");

        fx.set_param(FREEZE, 1.0);
        let (_, _) = run_blocks(&mut fx, &x[..24000], &x[..24000], 256);
        let silence = vec![0.0f32; 48000];
        let (held, _) = run_blocks(&mut fx, &silence, &silence, 256);
        assert!(rms(&held[24000..]) > 0.02, "{}", rms(&held[24000..]));
    }
}
