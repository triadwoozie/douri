// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! Picks the instrument or effect this build wraps: the registry id in `PROWAVE_PLUG_ID`
//! (the Pro Wave Macro Oscillator when it is not set), and writes its names and codes as
//! constants for the plug-in (`$OUT_DIR/selected.rs`). nih-plug makes one plug-in per library,
//! with its name and ids as constants, so each plug-in is a build of its own.

use std::fmt::Write as _;

mod codes {
    #![allow(dead_code)]
    include!("src/codes.rs");
}

/// The id built when `PROWAVE_PLUG_ID` is not set.
const DEFAULT_ID: &str = "prowave:macro-osc";

fn main() {
    println!("cargo:rerun-if-env-changed=PROWAVE_PLUG_ID");
    println!("cargo:rerun-if-changed=src/codes.rs");
    let id = std::env::var("PROWAVE_PLUG_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_ID.to_string());
    let (info, instrument) = match prowave_engine::native_instrument_info(&id) {
        Some(i) => (i, true),
        None => match prowave_engine::native_info(&id) {
            Some(i) => (i, false),
            None => panic!("PROWAVE_PLUG_ID={id}: no instrument or effect has this id"),
        },
    };
    let name = codes::bundle_name(info.name);
    let mut s = String::new();
    let _ = writeln!(s, "pub const ID: &str = {:?};", info.id);
    let _ = writeln!(s, "pub const NAME: &str = {name:?};");
    let _ = writeln!(s, "pub const INSTRUMENT: bool = {instrument};");
    let _ = writeln!(
        s,
        "pub const CLAP_ID: &str = {:?};",
        codes::clap_id(info.id)
    );
    let _ = writeln!(
        s,
        "pub const VST3_CLASS_ID: [u8; 16] = {:?};",
        codes::vst3_class_id(info.id)
    );
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(out.join("selected.rs"), s).expect("write selected.rs");
}
