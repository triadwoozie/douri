// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! Pro Wave Texture's tests. The comparison with the original C++ is in
//! `tests/texture_parity.rs`.

use super::dsp::Frame;
use super::grain::{Parameters, RandomizedParameter};
use super::processor::{Media, Processor, Settings};
use super::trigger::SeedState;
use super::*;
use crate::native_fx::test_signals::{noise, peak, rms, run_blocks, sine};

fn settings(density: f32, size: f32, pitch: f32) -> Settings {
    let rp = |base: f32| RandomizedParameter {
        base,
        attenuverter: 0.0,
    };
    Settings {
        density,
        params: Parameters {
            time: rp(0.3),
            size: rp(size),
            shape: rp(0.5),
            pitch: rp(pitch),
        },
        feedback: 0.0,
        dry_wet: 1.0,
        reverb: 0.0,
    }
}

/// Runs the engine on `input` (stereo, the same both sides) at its own rate.
fn run_engine(p: &mut Processor, s: &Settings, input: &[f32], freeze: bool) -> Vec<f32> {
    let block = p.media().block_size();
    let mut out = Vec::with_capacity(input.len());
    let fr = vec![freeze; block];
    for chunk in input.chunks(block) {
        let inp: Vec<Frame> = chunk.iter().map(|x| Frame::new(*x, *x)).collect();
        let mut o = vec![Frame::ZERO; chunk.len()];
        p.process(SeedState::Locked, &fr[..chunk.len()], s, &inp, &mut o);
        out.extend(o.iter().map(|f| 0.5 * (f.l + f.r)));
    }
    out
}

/// The power of `x` at `hz` (Goertzel).
fn power_at(x: &[f32], hz: f64, rate: f64) -> f64 {
    let w = std::f64::consts::TAU * hz / rate;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for v in x {
        let s0 = f64::from(*v) + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - c * s1 * s2) / (x.len() as f64 * x.len() as f64)
}

/// The frequency of `x` measured inside the grains: in each 50 ms window that is at least
/// half as loud as the loudest, the time between the first and last upward zero crossing
/// (interpolated) over the crossings counted; the median over the windows.
fn grain_frequency(x: &[f32], rate: f64) -> f64 {
    let win = (rate * 0.05) as usize;
    let windows: Vec<&[f32]> = x.chunks_exact(win).collect();
    let loudest = windows.iter().map(|w| rms(w)).fold(0.0f32, f32::max);
    let mut found: Vec<f64> = windows
        .iter()
        .filter(|w| rms(w) > 0.5 * loudest)
        .filter_map(|w| {
            let ups: Vec<f64> = (1..w.len())
                .filter(|&i| w[i - 1] < 0.0 && w[i] >= 0.0)
                .map(|i| {
                    let (a, b) = (f64::from(w[i - 1]), f64::from(w[i]));
                    (i - 1) as f64 + a / (a - b)
                })
                .collect();
            (ups.len() >= 3).then(|| (ups.len() - 1) as f64 * rate / (ups[ups.len() - 1] - ups[0]))
        })
        .collect();
    found.sort_by(f64::total_cmp);
    found[found.len() / 2]
}

/// One grain at a time: square grains of about 0.95 s, 0.72 a second.
fn single_grains(pitch: f32) -> Settings {
    let mut s = settings(0.47, 0.85, pitch);
    s.params.shape.base = 0.0;
    s
}

#[test]
fn pitch_shifts_the_grains_by_the_semitone_ratio() {
    let rate = 48000.0;
    let n = 6 * 48000;
    let input = sine(n, 0.5, 440.0, rate);
    for (semitones, label) in [
        (7.0f32, "fifth up"),
        (-12.0, "octave down"),
        (12.0, "octave up"),
        (0.0, "unison"),
    ] {
        let mut p = Processor::new(Media::Digital, SEED);
        let out = run_engine(&mut p, &single_grains(semitones), &input, false);
        let got = grain_frequency(&out[2 * 48000..], rate) / 440.0;
        let want = 2f64.powf(f64::from(semitones) / 12.0);
        println!(
            "{label}: ratio {got:.5}, expected {want:.5} ({:+.3}%)",
            (got / want - 1.0) * 100.0
        );
        assert!(
            (got / want - 1.0).abs() < 0.001,
            "{label}: ratio {got} expected {want}"
        );
    }
}

#[test]
fn pitch_through_the_effect_at_44100_keeps_the_ratio() {
    // The project-rate wrapper (44.1 kHz in, the engine at 48 kHz) must not move the pitch.
    let rate = 44100;
    let n = 6 * rate as usize;
    let x = sine(n, 0.5, 440.0, f64::from(rate));
    let mut values = PARAMS.map(|p| p.default);
    values[DENSITY] = 470.0;
    values[SIZE] = 850.0;
    values[SHAPE] = 0.0;
    values[DRY_WET] = 1000.0;
    values[REVERB] = 0.0;
    values[PITCH] = 700.0; // +7 semitones on the pot law
    assert_eq!(pitch_law(0.7), 7.0);
    let mut fx = Texture::new(rate, &values);
    let (l, r) = run_blocks(&mut fx, &x, &x, 512);
    let mono: Vec<f32> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect();
    let want = 2f64.powf(7.0 / 12.0);
    let got = grain_frequency(&mono[2 * rate as usize..], f64::from(rate)) / 440.0;
    println!(
        "44.1 kHz: ratio {got:.5}, expected {want:.5} ({:+.3}%)",
        (got / want - 1.0) * 100.0
    );
    assert!((got / want - 1.0).abs() < 0.001, "{got}");
    assert_eq!(fx.underruns, 0);
}

#[test]
fn freeze_holds_the_buffer() {
    let rate = 48000.0;
    let n = 2 * 48000;
    let a = sine(n, 0.5, 300.0, rate);
    let b = sine(n, 0.5, 1000.0, rate);
    let mut p = Processor::new(Media::Digital, SEED);
    let s = settings(0.17, 0.7, 0.0);
    run_engine(&mut p, &s, &a, false);
    let before = p.buffer_checksum();
    let out = run_engine(&mut p, &s, &b, true);
    assert_eq!(
        p.buffer_checksum(),
        before,
        "the buffer changed while frozen"
    );
    // Wet only: the grains still play the 300 Hz recording, not the 1 kHz input.
    let tail = &out[n / 4..];
    let held = power_at(tail, 300.0, rate);
    let new = power_at(tail, 1000.0, rate);
    println!("frozen: 300 Hz {held:.3e}, 1 kHz {new:.3e}");
    assert!(held > 1e-4 && new < held * 1e-3, "held {held} new {new}");
    // Unfrozen, the new input is recorded again.
    let out = run_engine(&mut p, &s, &b, false);
    assert_ne!(p.buffer_checksum(), before);
    let tail = &out[3 * n / 4..];
    assert!(power_at(tail, 1000.0, rate) > 100.0 * power_at(tail, 300.0, rate));
}

#[test]
fn freeze_holds_the_buffer_in_the_effect() {
    let rate = 44100;
    let n = rate as usize * 2;
    let mut values = PARAMS.map(|p| p.default);
    values[DRY_WET] = 1000.0;
    values[REVERB] = 0.0;
    values[DENSITY] = 170.0;
    let mut fx = Texture::new(rate, &values);
    let a = sine(n, 0.5, 300.0, f64::from(rate));
    let b = sine(n, 0.5, 1000.0, f64::from(rate));
    run_blocks(&mut fx, &a, &a, 256);
    fx.set_param(FREEZE, 1.0);
    let before = fx.processor.buffer_checksum();
    let (l, _) = run_blocks(&mut fx, &b, &b, 256);
    assert_eq!(fx.processor.buffer_checksum(), before);
    let tail = &l[n / 4..];
    let r = f64::from(rate);
    assert!(power_at(tail, 1000.0, r) < 1e-3 * power_at(tail, 300.0, r));
}

/// Every parameter at its minimum, default and maximum, every quality mode, every common
/// rate: loud noise and silence give finite output, nothing allocates and the output is
/// always ready.
#[test]
fn finite_at_every_extreme_and_rate_without_allocation() {
    for rate in [44100u32, 48000, 88200, 96000, 176400, 192000] {
        let n = rate as usize / 3;
        let (nl, nr) = (noise(n, 2.0, 3), noise(n, 2.0, 5));
        let silence = vec![0.0f32; n];
        for q in 0..4 {
            for setting in 0..4 {
                let mut values: Vec<f32> = PARAMS
                    .iter()
                    .map(|p| match setting {
                        0 => p.min,
                        1 => p.default,
                        2 => p.max,
                        // Bipolar knobs at their other end: random amounts fully negative.
                        _ => {
                            if p.min < 0.0 {
                                p.min
                            } else {
                                p.max
                            }
                        }
                    })
                    .collect();
                values[QUALITY] = q as f32;
                values[FREEZE] = if setting == 2 { 1.0 } else { 0.0 };
                let mut fx = Texture::new(rate, &values);
                for (l, r) in [(&nl, &nr), (&silence, &silence), (&nl, &nr)] {
                    let (ol, or) = run_blocks(&mut fx, l, r, 1024);
                    assert!(
                        ol.iter().chain(&or).all(|x| x.is_finite()),
                        "rate {rate} quality {q} setting {setting}"
                    );
                    assert!(peak(&ol).max(peak(&or)) < 4.0, "rate {rate} quality {q}");
                }
                assert_eq!(fx.underruns, 0, "rate {rate} quality {q}");
            }
        }
    }
}

#[test]
fn quality_switch_and_settings_do_not_allocate() {
    let rate = 96000;
    let n = rate as usize / 4;
    let x = noise(n, 0.5, 9);
    let mut fx = Texture::new(rate, &[]);
    for q in [3.0f32, 1.0, 2.0, 0.0] {
        assert_no_alloc::assert_no_alloc(|| {
            fx.set_param(QUALITY, q);
            fx.set_param(SIZE, 1000.0);
            fx.set_param(FREEZE, 1.0);
        });
        let (l, _) = run_blocks(&mut fx, &x, &x, 333);
        assert!(l.iter().all(|v| v.is_finite()));
        assert_eq!(fx.media(), media_of(q as i32));
        assert_no_alloc::assert_no_alloc(|| fx.set_param(FREEZE, 0.0));
    }
    assert_eq!(fx.underruns, 0);
}

/// The reported latency is where the dry signal comes out, in every mode and at every rate.
#[test]
fn reported_latency_is_the_dry_delay() {
    for rate in [44100u32, 48000, 96000, 192000] {
        for q in 0..4 {
            let mut values = PARAMS.map(|p| p.default);
            values[QUALITY] = q as f32;
            values[DRY_WET] = 0.0;
            values[REVERB] = 0.0;
            let mut fx = Texture::new(rate, &values);
            // A click after half a second (the limiters settle on silence first).
            let n = rate as usize;
            let at = n / 2;
            let mut x = vec![0.0f32; n];
            x[at] = 0.5;
            let (l, _) = run_blocks(&mut fx, &x, &x, 512);
            let (k, _) = l.iter().enumerate().fold((0, 0.0f32), |b, (i, v)| {
                if v.abs() > b.1 {
                    (i, v.abs())
                } else {
                    b
                }
            });
            let delay = k as i64 - at as i64;
            let reported = fx.latency_samples() as i64;
            // The dirty tape's wow and flutter is a moving delay of 1 to 123 samples at 24 kHz
            // on everything, dry signal included, as on the module.
            let (lo, hi) = if media_of(q) == Media::DirtyTape {
                (
                    reported,
                    reported + (f64::from(rate) * 124.0 / 24000.0).ceil() as i64,
                )
            } else {
                (reported - 1, reported + 1)
            };
            println!("rate {rate} quality {q}: reported {reported}, measured {delay}");
            assert!(
                delay >= lo && delay <= hi,
                "rate {rate} quality {q}: reported {reported}, measured {delay}"
            );
        }
    }
}

#[test]
fn two_runs_of_the_same_input_are_identical() {
    let rate = 48000;
    let n = rate as usize;
    let (l, r) = (noise(n, 0.7, 11), noise(n, 0.7, 12));
    let mut values = PARAMS.map(|p| p.default);
    values[DENSITY] = 900.0;
    values[TIME_RANDOM] = 800.0;
    values[PITCH_RANDOM] = -700.0;
    values[QUALITY] = 3.0;
    let mut a = Texture::new(rate, &values);
    let mut b = Texture::new(rate, &values);
    let oa = run_blocks(&mut a, &l, &r, 512);
    let ob = run_blocks(&mut b, &l, &r, 128);
    assert_eq!(oa, ob);
    assert!(rms(&oa.0) > 0.01);
}

#[test]
fn delay_mode_comes_in_at_full_size() {
    let mut p = Processor::new(Media::Digital, SEED);
    let s = settings(0.25, 1.0, 0.0);
    let x = noise(48000 * 2, 0.5, 1);
    run_engine(&mut p, &s, &x, false);
    assert!(p.delay_mode());
    let s = settings(0.25, 0.5, 0.0);
    run_engine(&mut p, &s, &x, false);
    assert!(!p.delay_mode());
}

#[test]
fn value_texts_and_laws() {
    assert_eq!(value_text(SIZE, 1000), "Delay");
    assert_eq!(value_text(DENSITY, 500), "Off");
    assert!(value_text(DENSITY, 300).ends_with("Hz"));
    assert!(value_text(DENSITY, 800).ends_with("RND"));
    assert!(value_text(SIZE, 100).starts_with('-'));
    assert_eq!(value_text(PITCH, 850), "+12.0 st");
    assert_eq!(value_text(PITCH, -1000), "-24.0 st");
    assert_eq!(value_text(TIME_RANDOM, 0), "0%");
    assert_eq!(value_text(TIME_RANDOM, 1000), "+100%");
    assert_eq!(value_text(QUALITY, 3), "Dirty Tape");
    assert!((attenuverter_law(1.0) - 1.0).abs() < 1e-6);
    assert_eq!(attenuverter_law(0.05), 0.0);
    // Grain length: 4 s at full size short of the delay mode, under 10 ms at a third.
    assert!(grain_seconds(989).is_some_and(|s| s > 3.5));
    assert!(grain_seconds(334).is_some_and(|s| s < 0.01));
    assert_eq!(envelope_points(500, 64).len(), 64);
    for p in &PRESETS {
        assert!(p.values.iter().all(|(k, v)| {
            let q = &PARAMS[*k];
            *v >= q.min && *v <= q.max
        }));
    }
    // The Stutter Delay preset: DENSITY 0.25 repeats every 4 s / 2^4.5 = 177 ms.
    let (s, k) = delay_repeat(0, 250, 0);
    assert_eq!(k, 1);
    assert!((s - 4.0 / 2f32.powf(4.5)).abs() < 0.001, "{s}");
    assert_eq!(delay_repeat(0, 250, 1000).1, 16);
    for m in Media::ALL {
        assert!(m.buffer_seconds() >= 4.0, "{m:?}");
    }
}
