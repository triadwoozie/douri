// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

// How a registry id becomes a plug-in's names and codes. All of them are made from the id
// alone, so they stay the same from one build to the next. This file is also included by
// build.rs (`include!`), so it uses nothing but the standard library.

/// The name every plug-in starts with, so a host lists them together.
pub const PREFIX: &str = "Pro Wave";

/// The plug-in's name (and its bundles' file name): the registry name with "Pro Wave " in front
/// when it does not start with it already ("Kick" is "Pro Wave Kick", "Pro Wave Tune" stays).
pub fn bundle_name(name: &str) -> String {
    if name.starts_with("Pro Wave ") {
        name.to_string()
    } else {
        format!("{PREFIX} {name}")
    }
}

/// The id without "prowave:".
pub fn slug(id: &str) -> &str {
    id.strip_prefix("prowave:").unwrap_or(id)
}

/// The CLAP plug-in id: "com.yuribeats.prowave." and the slug.
pub fn clap_id(id: &str) -> String {
    format!("com.yuribeats.prowave.{}", slug(id))
}

/// The bundles' identifier: the CLAP id and the format ("…kick.vst3").
pub fn bundle_id(id: &str, format: &str) -> String {
    format!("{}.{format}", clap_id(id))
}

/// FNV-1a, 64 bits, from `seed`.
fn fnv1a64(seed: u64, bytes: &[u8]) -> u64 {
    let mut h = seed;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// The VST3 class id: two FNV-1a hashes of the id, with different prefixes.
pub fn vst3_class_id(id: &str) -> [u8; 16] {
    let a = fnv1a64(FNV_OFFSET, format!("prowave-plug/vst3/a/{id}").as_bytes());
    let b = fnv1a64(FNV_OFFSET, format!("prowave-plug/vst3/b/{id}").as_bytes());
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&a.to_be_bytes());
    out[8..].copy_from_slice(&b.to_be_bytes());
    out
}

/// The Audio Unit manufacturer code (one capital letter at least, as Apple asks).
pub const AU_MANUFACTURER: &str = "PrWv";
/// The Audio Unit manufacturer name.
pub const AU_MANUFACTURER_NAME: &str = "Pro Wave";

/// Effects that also take MIDI notes (Pro Wave Tune: the notes held are its targets in MIDI
/// mode). Their VST3 and CLAP have a note input, and their Audio Unit is a music effect.
pub fn takes_notes(id: &str) -> bool {
    id == "prowave:tune"
}

/// The Audio Unit type: an instrument is a music device; an effect that takes notes a music
/// effect (`aumf`, which Logic lists with the effects and gives a MIDI side-chain menu); any
/// other effect an effect.
pub fn au_type(id: &str, instrument: bool) -> &'static str {
    if instrument {
        "aumu"
    } else if takes_notes(id) {
        "aumf"
    } else {
        "aufx"
    }
}

/// The Audio Unit subtype: four characters from a hash of the id, a lower-case letter and
/// three letters or digits. The crate's tests check that no two plug-ins share one.
pub fn au_subtype(id: &str) -> String {
    const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    const ALNUM: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut h = fnv1a64(FNV_OFFSET, format!("prowave-plug/au/{id}").as_bytes());
    let mut out = String::with_capacity(4);
    out.push(LETTERS[(h % 26) as usize] as char);
    h /= 26;
    for _ in 0..3 {
        out.push(ALNUM[(h % 36) as usize] as char);
        h /= 36;
    }
    out
}
