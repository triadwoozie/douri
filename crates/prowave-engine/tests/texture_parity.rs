//! Pro Wave Texture against the original: Mutable Instruments' Beads processor (the C++ in
//! tests/texture_reference/eurorack, MIT, built here with the Mac's C++ compiler) and the
//! Rust port run the same blocks of input with the same settings and the same random seed,
//! and must give the same samples (within `TOLERANCE`).
//!
//! macOS only: the port reproduces the tie order of `std::partial_sort` as LLVM's libc++
//! does it (Apple's C++ library); GNU's library orders ties differently.
//! Needs a C++ compiler (`PROWAVE_CXX`, else `c++`).
#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use prowave_engine::texture::dsp::Frame;
use prowave_engine::texture::grain::{Parameters, RandomizedParameter};
use prowave_engine::texture::processor::{Media, Processor, Settings};
use prowave_engine::texture::trigger::SeedState;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The largest difference allowed between the two, per sample.
const TOLERANCE: f32 = 1e-5;

fn reference_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/texture_reference")
}

/// Builds and runs the C++ harness; returns the folder with one `.bin` per case.
fn run_reference(out: &Path) {
    let src = reference_dir();
    let cxx = std::env::var("PROWAVE_CXX").unwrap_or_else(|_| "c++".into());
    let exe = out.join("harness");
    let mut files = vec![src.join("harness.cc")];
    for f in [
        "beads/dsp/processor.cc",
        "beads/dsp/delay_controller.cc",
        "beads/dsp/delay_clock.cc",
        "beads/dsp/trigger_generator.cc",
        "beads/dsp/mu_law.cc",
        "beads/resources.cc",
        "stmlib/utils/random.cc",
        "stmlib/dsp/units.cc",
    ] {
        files.push(src.join("eurorack").join(f));
    }
    let status = Command::new(&cxx)
        .args(["-std=c++14", "-O2", "-DTEST", "-ffp-contract=off", "-w"])
        .arg("-I")
        .arg(src.join("eurorack"))
        .args(&files)
        .arg("-o")
        .arg(&exe)
        .status()
        .unwrap_or_else(|e| panic!("cannot run the C++ compiler {cxx}: {e}"));
    assert!(status.success(), "the C++ reference did not build");
    let status = Command::new(&exe).arg(out).status().unwrap();
    assert!(status.success(), "the C++ reference failed");
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn f32s(b: &[u8], at: usize, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| f32::from_le_bytes(b[at + 4 * i..at + 4 * i + 4].try_into().unwrap()))
        .collect()
}

struct Outcome {
    max_diff: f32,
    first_diff_block: Option<usize>,
    samples: usize,
    peak: f32,
}

fn compare(bytes: &[u8]) -> (Media, Outcome) {
    assert_eq!(&bytes[..4], b"PWTX");
    let media = match u32_at(bytes, 4) {
        0 => Media::DirtyTape,
        1 => Media::CleanTape,
        2 => Media::VintageDigital,
        _ => Media::Digital,
    };
    let block = u32_at(bytes, 8) as usize;
    let blocks = u32_at(bytes, 12) as usize;
    assert_eq!(block, media.block_size());
    let mut p = Processor::new(media, 0x21);
    let mut at = 16;
    let mut o = Outcome {
        max_diff: 0.0,
        first_diff_block: None,
        samples: 0,
        peak: 0.0,
    };
    for k in 0..blocks {
        let v = f32s(bytes, at, 13);
        at += 13 * 4;
        let input_raw = f32s(bytes, at, 2 * block);
        at += 8 * block;
        let expected = f32s(bytes, at, 2 * block);
        at += 8 * block;
        let rp = |i: usize| RandomizedParameter {
            base: v[1 + i],
            attenuverter: v[5 + i],
        };
        let s = Settings {
            density: v[0],
            params: Parameters {
                time: rp(0),
                size: rp(1),
                shape: rp(2),
                pitch: rp(3),
            },
            feedback: v[9],
            dry_wet: v[10],
            reverb: v[11],
        };
        let freeze = vec![v[12] > 0.5; block];
        let input: Vec<Frame> = (0..block)
            .map(|i| Frame::new(input_raw[2 * i], input_raw[2 * i + 1]))
            .collect();
        let mut out = vec![Frame::ZERO; block];
        p.process(SeedState::Locked, &freeze, &s, &input, &mut out);
        for i in 0..block {
            for (got, want) in [(out[i].l, expected[2 * i]), (out[i].r, expected[2 * i + 1])] {
                let d = (got - want).abs();
                assert!(got.is_finite());
                o.peak = o.peak.max(want.abs());
                if d > o.max_diff {
                    o.max_diff = d;
                }
                if d > 0.0 && o.first_diff_block.is_none() {
                    o.first_diff_block = Some(k);
                }
            }
        }
        o.samples += block;
    }
    (media, o)
}

#[test]
fn texture_matches_the_beads_cpp_on_every_case() {
    let dir = tempfile::tempdir().unwrap();
    run_reference(dir.path());
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 12, "cases written by the harness");
    let mut failures = Vec::new();
    for path in names {
        let bytes = std::fs::read(&path).unwrap();
        let (media, o) = compare(&bytes);
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        println!(
            "{name:24} {media:?}: {} frames, peak {:.3}, max difference {:.3e}, first different block {:?}",
            o.samples, o.peak, o.max_diff, o.first_diff_block
        );
        assert!(o.peak > 0.01, "{name}: the reference is silent");
        if o.max_diff > TOLERANCE {
            failures.push(format!("{name}: {:.3e}", o.max_diff));
        }
    }
    assert!(failures.is_empty(), "over {TOLERANCE}: {failures:?}");
}
