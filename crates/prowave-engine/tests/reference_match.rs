//! What reference-matched mastering does to a song: its spectrum moves to the reference's, its
//! loudness lands on the reference's, the limiter keeps the ceiling, mono and other rates work.
//! (`reference_match_parity.rs` holds the port against the Python original.)
//!
//! The test songs are pink noise high-passed at 25 Hz. Matchering matches plain RMS, which
//! counts what is under 20 Hz while LUFS do not; unfiltered pink noise (strong down to a few
//! Hz) gives results about 0.4 LU louder than their reference.
#![allow(clippy::unwrap_used)]

mod reference_match_signals;

use prowave_engine::reference_match::{self as rm, loudness, Ceiling, Config, Finish, Input};
use reference_match_signals::*;

const RATE: u32 = 44_100;

fn song(seed: u64, rate: u32, seconds: f64, tilt_by: f64, width: f64, gain: f64) -> Input {
    let n = (seconds * f64::from(rate)) as usize;
    let mut mid = tilt(&pink(seed, n), tilt_by);
    highpass(&mut mid, rate, 25.0);
    shape(&mut mid, rate, 3.0, &[0.5, 1.0, 0.8, 0.35, 0.9]);
    let (mut l, mut r) = stereo(&mid, seed ^ 0x5151, width, gain);
    highpass(&mut l, rate, 25.0);
    highpass(&mut r, rate, 25.0);
    Input {
        channels: vec![l, r],
        rate,
    }
}

fn lufs(s: &rm::Stereo) -> f64 {
    loudness::integrated_lufs(&[&s.l, &s.r], RATE).unwrap()
}

/// The mid channel's level in dB in octave bands from 63 Hz to 16 kHz (averaged 4096-point
/// spectra).
fn octaves(s: &rm::Stereo) -> Vec<f64> {
    let n = 4096;
    let f = rm::fft::Fft::new(n);
    let mid: Vec<f64> = s.l.iter().zip(&s.r).map(|(a, b)| (a + b) * 0.5).collect();
    let mut power = vec![0.0; n / 2 + 1];
    let (mut re, mut im) = (vec![0.0; n], vec![0.0; n]);
    for b in 0..mid.len() / n {
        re.copy_from_slice(&mid[b * n..(b + 1) * n]);
        im.iter_mut().for_each(|v| *v = 0.0);
        f.run(&mut re, &mut im, false);
        for k in 0..=n / 2 {
            power[k] += re[k] * re[k] + im[k] * im[k];
        }
    }
    let hz = |k: usize| k as f64 * f64::from(RATE) / n as f64;
    (0..9)
        .map(|o| {
            let c = 62.5 * 2f64.powi(o);
            let (lo, hi) = (c / 2f64.sqrt(), c * 2f64.sqrt());
            let p: f64 = (1..=n / 2)
                .filter(|&k| hz(k) >= lo && hz(k) < hi)
                .map(|k| power[k])
                .sum();
            10.0 * p.max(1e-30).log10()
        })
        .collect()
}

/// The band levels with their mean taken off (the shape, not the level).
fn shape_of(bands: &[f64]) -> Vec<f64> {
    let mean = bands.iter().sum::<f64>() / bands.len() as f64;
    bands.iter().map(|b| b - mean).collect()
}

fn match_and_finish(
    target: Input,
    reference: Input,
    how: Finish,
) -> (rm::Stereo, rm::Stereo, rm::Stereo) {
    let cfg = Config::default();
    let (t, r, _) = rm::prepare(target, reference, &cfg).unwrap();
    let m = rm::match_to(&t, &r, &cfg, &mut |_| true).unwrap();
    let out = rm::finish(&m, &how, &cfg, &mut |_| true).unwrap();
    (t, r, out)
}

#[test]
fn pink_noise_takes_the_tilt_of_its_reference() {
    // A pink target against a reference tilted bright: the result's spectrum follows the tilt.
    let (t, r, out) = match_and_finish(
        song(11, RATE, 24.0, 0.0, 0.2, 0.5),
        song(12, RATE, 24.0, 1.5, 0.2, 0.9),
        Finish {
            limiter: false,
            normalize: false,
            ceiling: Ceiling::Matchering,
        },
    );
    let (ts, rs, os) = (
        shape_of(&octaves(&t)),
        shape_of(&octaves(&r)),
        shape_of(&octaves(&out)),
    );
    let before = ts
        .iter()
        .zip(&rs)
        .fold(0.0f64, |m, (a, b)| m.max((a - b).abs()));
    let after = os
        .iter()
        .zip(&rs)
        .fold(0.0f64, |m, (a, b)| m.max((a - b).abs()));
    println!("octave shape vs reference: target off by up to {before:.2} dB, result {after:.2} dB");
    assert!(before > 6.0, "the reference is tilted ({before:.2} dB)");
    assert!(after < 1.0, "the result follows it to {after:.2} dB");
}

/// With the limiter, as Matchering's default. (Without it the result is not meant to be as
/// loud as the reference: Matchering matches it to the reference brought up to full scale,
/// with its peaks over full scale left for a limiter after it.)
#[test]
fn the_loudness_lands_on_the_references() {
    for (name, how) in [
        ("limiter", Finish::default()),
        (
            "true-peak limiter at -1 dBTP",
            Finish {
                limiter: true,
                normalize: false,
                ceiling: Ceiling::TruePeak(-1.0),
            },
        ),
    ] {
        let (t, r, out) = match_and_finish(
            song(21, RATE, 24.0, -0.5, 0.15, 0.25),
            song(22, RATE, 24.0, 0.7, 0.3, 0.55),
            how,
        );
        let (lt, lr, lo) = (lufs(&t), lufs(&r), lufs(&out));
        println!("{name}: target {lt:.2} LUFS, reference {lr:.2}, result {lo:.2}");
        assert!((lt - lr).abs() > 4.0, "the target starts apart");
        assert!((lo - lr).abs() <= 0.5, "{name}: {lo:.2} vs {lr:.2} LUFS");
    }
}

#[test]
fn the_limiter_keeps_the_true_peak_at_the_ceiling() {
    for ceiling in [-1.0, -0.3] {
        let (_, _, out) = match_and_finish(
            song(31, RATE, 12.0, 0.3, 0.25, 0.4),
            // A hot reference, so the result is pushed into the limiter.
            song(32, RATE, 12.0, 0.9, 0.3, 1.6),
            Finish {
                limiter: true,
                normalize: false,
                ceiling: Ceiling::TruePeak(ceiling),
            },
        );
        let tp = loudness::db(loudness::true_peak(&[&out.l, &out.r]));
        let check = loudness::db(independent_true_peak(&out));
        let sample = loudness::db(
            out.l
                .iter()
                .chain(&out.r)
                .fold(0.0f64, |m, v| m.max(v.abs())),
        );
        println!("ceiling {ceiling} dBTP: true peak {tp:.4} dBTP (whole-song check {check:.4}), sample peak {sample:.3} dBFS");
        assert!(tp <= ceiling + 1e-9, "true peak {tp} over {ceiling}");
        assert!(
            check <= ceiling + 0.005,
            "the whole-song check reads {check}"
        );
    }
    // Matchering's own ceiling is a sample-peak one at its threshold.
    let (_, _, out) = match_and_finish(
        song(33, RATE, 20.0, 0.3, 0.25, 0.4),
        song(34, RATE, 20.0, 0.9, 0.3, 1.6),
        Finish::default(),
    );
    let peak = out
        .l
        .iter()
        .chain(&out.r)
        .fold(0.0f64, |m, v| m.max(v.abs()));
    assert!(peak <= Config::default().threshold + 1e-12, "{peak}");
}

/// True peak measured another way: the whole channel's spectrum zero-padded to 8 times its
/// length (the band-limited signal between the samples, no blocks), the top of each peak
/// found with a parabola through the points around it.
fn independent_true_peak(s: &rm::Stereo) -> f64 {
    let mut best = 0.0f64;
    for x in [&s.l, &s.r] {
        let n = x.len().next_power_of_two() * 2;
        let os = 8;
        let mut re = vec![0.0; n];
        let mut im = vec![0.0; n];
        re[..x.len()].copy_from_slice(x);
        rm::fft::Fft::new(n).run(&mut re, &mut im, false);
        let big = n * os;
        let mut ur = vec![0.0; big];
        let mut ui = vec![0.0; big];
        let h = n / 2;
        ur[..h].copy_from_slice(&re[..h]);
        ui[..h].copy_from_slice(&im[..h]);
        for k in 1..h {
            ur[big - k] = re[n - k];
            ui[big - k] = im[n - k];
        }
        rm::fft::Fft::new(big).run(&mut ur, &mut ui, true);
        for q in 1..big - 1 {
            let (a, b, c) = (ur[q - 1].abs(), ur[q].abs(), ur[q + 1].abs());
            let mut v = b;
            let den = a - 2.0 * b + c;
            if b >= a && b >= c && den < 0.0 {
                let p = 0.5 * (a - c) / den;
                v = b - 0.25 * (a - c) * p;
            }
            best = best.max(v * os as f64);
        }
    }
    best
}

#[test]
fn a_mono_target_comes_out_as_two_equal_channels() {
    let mono = song(41, RATE, 15.0, 0.0, 0.0, 0.5).channels.remove(0);
    let (t, _, out) = match_and_finish(
        Input {
            channels: vec![mono],
            rate: RATE,
        },
        song(42, RATE, 15.0, 0.5, 0.3, 0.8),
        Finish::default(),
    );
    assert_eq!(t.l, t.r);
    assert_eq!(out.len(), t.len());
    assert!(out.l.iter().zip(&out.r).all(|(a, b)| (a - b).abs() < 1e-12));
}

#[test]
fn other_sample_rates_are_converted() {
    // A 48 kHz target and a 96 kHz reference: the result is at 44.1 kHz, as long as the
    // target, and as loud as the reference.
    let (t, r, out) = match_and_finish(
        song(51, 48_000, 12.0, 0.2, 0.2, 0.3),
        song(52, 96_000, 12.0, -0.3, 0.3, 0.7),
        Finish::default(),
    );
    assert_eq!(t.len(), (12.0 * 48_000.0 * 44_100.0 / 48_000.0) as usize);
    assert_eq!(out.len(), t.len());
    assert_eq!(r.len(), 12 * 44_100);
    let (lr, lo) = (lufs(&r), lufs(&out));
    assert!((lo - lr).abs() <= 0.5, "{lo:.2} vs {lr:.2}");
}

#[test]
fn checks_refuse_what_matchering_refuses() {
    let cfg = Config::default();
    let short = Input {
        channels: vec![vec![0.1; 1000]],
        rate: RATE,
    };
    assert_eq!(
        rm::prepare(short.clone(), song(1, RATE, 5.0, 0.0, 0.1, 0.5), &cfg).unwrap_err(),
        rm::MatchError::TargetTooShort
    );
    let s = song(2, RATE, 5.0, 0.0, 0.1, 0.5);
    assert_eq!(
        rm::prepare(s.clone(), s, &cfg).unwrap_err(),
        rm::MatchError::TargetEqualsReference
    );
    let three = Input {
        channels: vec![vec![0.1; 10_000]; 3],
        rate: RATE,
    };
    assert_eq!(
        rm::prepare(three, song(3, RATE, 5.0, 0.0, 0.1, 0.5), &cfg).unwrap_err(),
        rm::MatchError::TargetChannels
    );
    // Cancelling stops it.
    let (t, r, _) = rm::prepare(
        song(4, RATE, 10.0, 0.0, 0.1, 0.5),
        song(5, RATE, 10.0, 0.4, 0.2, 0.7),
        &cfg,
    )
    .unwrap();
    let mut calls = 0;
    let res = rm::match_to(&t, &r, &cfg, &mut |_| {
        calls += 1;
        calls < 3
    });
    assert_eq!(res.unwrap_err(), rm::MatchError::Cancelled);
}

/// How long a four-minute song takes, stage by stage (release build):
/// `cargo test --release -p prowave-engine --test reference_match timing -- --ignored --nocapture`.
#[test]
#[ignore = "a timing, for a release build"]
fn timing_of_a_four_minute_song() {
    let cfg = Config::default();
    let t0 = std::time::Instant::now();
    let (t, r, _) = rm::prepare(
        song(61, 48_000, 240.0, -0.3, 0.2, 0.3),
        song(62, RATE, 240.0, 0.5, 0.3, 0.8),
        &cfg,
    )
    .unwrap();
    let prepared = t0.elapsed();
    let m = rm::match_to(&t, &r, &cfg, &mut |_| true).unwrap();
    let matched = t0.elapsed();
    let _ = rm::finish(&m, &Finish::default(), &cfg, &mut |_| true).unwrap();
    let limited = t0.elapsed();
    let tp = Finish {
        limiter: true,
        normalize: false,
        ceiling: Ceiling::TruePeak(-1.0),
    };
    let out = rm::finish(&m, &tp, &cfg, &mut |_| true).unwrap();
    let true_peak = t0.elapsed();
    let _ = lufs(&out);
    let _ = loudness::true_peak(&[&out.l, &out.r]);
    let measured = t0.elapsed();
    println!(
        "TIMING 240 s: prepare (48k resample) {:.2} s, match {:.2} s, Matchering limiter {:.2} s, true-peak limiter {:.2} s, LUFS + true peak {:.2} s",
        prepared.as_secs_f64(),
        (matched - prepared).as_secs_f64(),
        (limited - matched).as_secs_f64(),
        (true_peak - limited).as_secs_f64(),
        (measured - true_peak).as_secs_f64(),
    );
}
