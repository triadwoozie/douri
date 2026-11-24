// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! Prints every plug-in the public build makes, one line each, tab-separated: id, kind
//! ("instrument" or "effect"), plug-in name, CLAP id, AU type, AU subtype, AU manufacturer,
//! parameter count, preset count. tools/build-plugins.sh reads it.

use prowave_plug::codes;

fn main() {
    let all = prowave_engine::native_instruments()
        .iter()
        .map(|i| (i, true))
        .chain(prowave_engine::native_effects().iter().map(|i| (i, false)));
    for (info, instrument) in all {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            info.id,
            if instrument { "instrument" } else { "effect" },
            codes::bundle_name(info.name),
            codes::clap_id(info.id),
            codes::au_type(instrument),
            codes::au_subtype(info.id),
            codes::AU_MANUFACTURER,
            info.params.len(),
            info.presets.len()
        );
    }
}
