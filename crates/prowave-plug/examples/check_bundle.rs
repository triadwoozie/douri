// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! Checks built `.clap` bundles without a host program or a sound card: for each bundle given,
//! opens its library, makes the plug-in through its CLAP entry, lists its parameters, and
//! renders two seconds offline (a chord for an instrument, a test signal through an effect)
//! at 48 kHz in blocks of odd sizes. Prints one line per bundle and fails when the output is
//! silent or not finite.
//!
//!   cargo run --example check_bundle -- path/to/One.clap path/to/Two.clap

#[path = "../tests/common/clap_host.rs"]
mod clap_host;

use clap_host::{instrument_score, peak, render, rms, test_signal, Host};
use std::ffi::CString;
use std::path::Path;

fn check(bundle: &Path) -> Result<String, String> {
    let name = bundle
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("no name")?;
    let lib = bundle.join("Contents/MacOS").join(name);
    // SAFETY: the library is a plug-in built from this crate; it stays loaded (leaked) for
    // the rest of the run, as a host keeps it.
    let lib = unsafe { libloading::Library::new(&lib) }.map_err(|e| format!("open: {e}"))?;
    let lib: &'static libloading::Library = Box::leak(Box::new(lib));
    // SAFETY: `clap_entry` is the CLAP entry point every CLAP library exports.
    let entry: &'static clap_sys::entry::clap_plugin_entry = unsafe {
        let sym = lib
            .get::<*const clap_sys::entry::clap_plugin_entry>(b"clap_entry\0")
            .map_err(|e| format!("clap_entry: {e}"))?;
        &**sym
    };
    let path = CString::new(bundle.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let rate = 48_000.0;
    let mut h = Host::new(entry, &path, rate, 4096);
    let instrument = h.inputs == 0;
    let score = if instrument {
        instrument_score()
    } else {
        Vec::new()
    };
    let texts: Vec<String> = (0..h.params.len().min(3))
        .map(|k| {
            format!(
                "{}={}",
                h.params[k].name,
                h.value_text(k, h.params[k].default)
            )
        })
        .collect();
    let (l, r) = render(
        &mut h,
        &score,
        2.0,
        &[512, 333, 4096, 64, 1],
        &mut test_signal(rate),
    );
    let finite = l.iter().chain(&r).all(|v| v.is_finite());
    let first = (1.2 * rate) as usize;
    let level = rms(&l[..first]).max(rms(&r[..first]));
    let line = format!(
        "{}\t{}\t{}\tparams={}\tlatency={}\tpeak={:.4}\trms={:.4}\tfinite={}\t{}",
        h.plugin_name,
        h.plugin_id,
        if instrument { "instrument" } else { "effect" },
        h.params.len(),
        h.latency(),
        peak(&l).max(peak(&r)),
        level,
        finite,
        texts.join("; ")
    );
    if !finite || level < 1e-3 {
        return Err(line);
    }
    Ok(line)
}

fn main() {
    let mut failed = 0;
    for arg in std::env::args().skip(1) {
        match check(Path::new(&arg)) {
            Ok(line) => println!("PASS\t{line}"),
            Err(line) => {
                println!("FAIL\t{arg}\t{line}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        eprintln!("{failed} bundle(s) failed");
        std::process::exit(1);
    }
}
