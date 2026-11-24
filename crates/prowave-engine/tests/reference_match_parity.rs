//! The Rust port of Matchering against the Python original on the same files.
//!
//! Needs Python with Matchering 2.0.6: `PROWAVE_MATCHERING_PYTHON` names its interpreter,
//! otherwise `uv run --with matchering==2.0.6` is used. Run with
//! `cargo test -p prowave-engine --test reference_match_parity -- --ignored --nocapture`.
//!
//! Each case writes a target and a reference (32-bit float WAV, so both sides read the same
//! numbers), runs `tools/matchering/reference.py` on them and `reference_match` here, and
//! compares the three finishes (limiter; no limiter; no limiter, normalised), the mid and side
//! FIRs and the gains.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod reference_match_signals;

use prowave_engine::reference_match::{self as rm, Ceiling, Config, Finish};
use reference_match_signals::*;
use std::path::Path;
use std::process::Command;

fn read_f64(path: &Path) -> Vec<f64> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    bytes
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| f64::from_le_bytes(*b))
        .collect()
}

fn python(target: &Path, reference: &Path, out: &Path) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/matchering/reference.py");
    let mut cmd = match std::env::var_os("PROWAVE_MATCHERING_PYTHON") {
        Some(py) => Command::new(py),
        None => {
            let mut c = Command::new("uv");
            c.args([
                "run",
                "--with",
                "matchering==2.0.6",
                "--python",
                "3.11",
                "python",
            ]);
            c
        }
    };
    let out_ = cmd
        .arg(&script)
        .arg(target)
        .arg(reference)
        .arg(out)
        .output()
        .expect("Python with Matchering is needed for this test (PROWAVE_MATCHERING_PYTHON or uv)");
    assert!(
        out_.status.success(),
        "reference.py failed: {}",
        String::from_utf8_lossy(&out_.stderr)
    );
}

/// The largest difference, and the largest value of `want`.
fn diff(got: &[f64], want: &[f64]) -> (f64, f64) {
    assert_eq!(got.len(), want.len(), "lengths differ");
    let d = got
        .iter()
        .zip(want)
        .fold(0.0f64, |m, (a, b)| m.max((a - b).abs()));
    let peak = want.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    (d, peak)
}

fn interleave(s: &rm::Stereo) -> Vec<f64> {
    s.l.iter().zip(&s.r).flat_map(|(a, b)| [*a, *b]).collect()
}

/// Largest difference allowed between the port's audio and the original's: -140 dBFS.
const AUDIO_TOL: f64 = 1e-7;

fn case(name: &str, target: (Vec<Vec<f64>>, u32), reference: (Vec<Vec<f64>>, u32)) {
    let dir = tempfile::tempdir().unwrap();
    let tp = dir.path().join("target.wav");
    let rp = dir.path().join("reference.wav");
    let t: Vec<&[f64]> = target.0.iter().map(Vec::as_slice).collect();
    let r: Vec<&[f64]> = reference.0.iter().map(Vec::as_slice).collect();
    write_f32_wav(&tp, &t, target.1);
    write_f32_wav(&rp, &r, reference.1);
    let out = dir.path().join("py");
    python(&tp, &rp, &out);

    let cfg = Config::default();
    let (ts, rs, _) = rm::prepare(read_input(&tp), read_input(&rp), &cfg).unwrap();
    let m = rm::match_to(&ts, &rs, &cfg, &mut |_| true).unwrap();
    let fin = |limiter, normalize| {
        let how = Finish {
            limiter,
            normalize,
            ceiling: Ceiling::Matchering,
        };
        interleave(&rm::finish(&m, &how, &cfg, &mut |_| true).unwrap())
    };

    let scalars = read_f64(&out.join("scalars.f64"));
    let ours: Vec<f64> = std::iter::once(m.final_amplitude)
        .chain(std::iter::once(m.rms_coefficient))
        .chain(m.corrections.iter().copied())
        .collect();
    assert_eq!(scalars.len(), ours.len(), "{name}: gains");
    let gain_err = scalars
        .iter()
        .zip(&ours)
        .fold(0.0f64, |e, (a, b)| e.max(((a - b) / a).abs()));

    let (mid_d, mid_p) = diff(&m.mid.fir, &read_f64(&out.join("mid_fir.f64")));
    let (side_d, side_p) = diff(&m.side.fir, &read_f64(&out.join("side_fir.f64")));
    let mut worst = 0.0f64;
    let mut lines = Vec::new();
    for (file, limiter, normalize) in [
        ("limited", true, true),
        ("nolimiter", false, false),
        ("normalized", false, true),
    ] {
        let want = read_f64(&out.join(format!("{file}.f64")));
        let (d, peak) = diff(&fin(limiter, normalize), &want);
        worst = worst.max(d);
        lines.push(format!(
            "{file}: max diff {d:.3e} ({:.1} dB below its peak {peak:.4})",
            20.0 * (peak / d.max(1e-300)).log10()
        ));
    }
    println!(
        "PARITY {name}: gains rel {gain_err:.2e}; mid FIR {mid_d:.2e} of {mid_p:.3}; side FIR {side_d:.2e} of {side_p:.3}; {}",
        lines.join("; ")
    );
    assert!(gain_err < 1e-9, "{name}: gains differ by {gain_err:e}");
    assert!(
        mid_d < 1e-9 * mid_p.max(1.0) && side_d < 1e-9 * side_p.max(1.0),
        "{name}: FIRs"
    );
    assert!(worst < AUDIO_TOL, "{name}: audio differs by {worst:e}");
}

fn song(seed: u64, rate: u32, seconds: f64, tilt_by: f64, width: f64, gain: f64) -> Vec<Vec<f64>> {
    let n = (seconds * f64::from(rate)) as usize;
    let mut mid = tilt(&pink(seed, n), tilt_by);
    shape(&mut mid, rate, 4.0, &[0.4, 1.0, 0.7, 0.25, 0.9]);
    let (l, r) = stereo(&mid, seed ^ 0xABCD, width, gain);
    vec![l, r]
}

#[test]
#[ignore = "needs Python with Matchering 2.0.6 (PROWAVE_MATCHERING_PYTHON or uv)"]
fn the_port_matches_matchering() {
    // Stereo at 44.1 kHz, three 15-second pieces against two.
    case(
        "stereo",
        (song(1, 44_100, 40.0, -0.6, 0.15, 0.6), 44_100),
        (song(2, 44_100, 33.0, 0.8, 0.45, 1.4), 44_100),
    );
    // A mono target.
    let mono = song(3, 44_100, 20.0, 0.0, 0.0, 0.8).remove(0);
    case(
        "mono target",
        (vec![mono], 44_100),
        (song(4, 44_100, 20.0, 0.5, 0.3, 1.2), 44_100),
    );
    // Rates other than 44.1 kHz: the target down from 48 kHz, the reference up from 32 kHz.
    case(
        "48k target, 32k reference",
        (song(5, 48_000, 18.0, -0.3, 0.2, 0.7), 48_000),
        (song(6, 32_000, 16.0, 0.6, 0.35, 1.3), 32_000),
    );
    // A reference that peaks well under full scale (the final amplitude coefficient).
    case(
        "quiet reference",
        (song(7, 44_100, 20.0, 0.2, 0.2, 1.0), 44_100),
        (song(8, 44_100, 20.0, -0.4, 0.25, 0.3), 44_100),
    );
}
