// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! How a plug-in's parameters are laid out: pages (the keys along the top of an instrument with
//! more parameters than one panel holds), each in rows of sections. The same layout as Pro
//! Wave's own windows: the pages of `prowave-app/src/native_inst_ui.rs` (`pages`, `rows_of`) and
//! the sections of `prowave-app/src/native_fx_ui.rs` (`layout`, `short_label`), copied here
//! because the program's window code is not a library. A page's key also names the parameter
//! group the host shows.

use prowave_engine::native_fx::{
    circuit, lemon_fast_distortion, lemon_limiter, lemon_soft_clipper, lemon_transient_processor,
    press, resonator_fx, spectral,
};
use prowave_engine::native_inst::{
    bass_line, chip_opl, chip_opll, chip_opn, chip_psg, chip_sid, digital_osc, drum_synth,
    engine_sound, fm6, kick, macro_osc, monstro, pluck, resonator, triple_osc, wave_draw,
};
use prowave_engine::NativeFxInfo;

/// Parameters on a numbered page.
const PAGE_PARAMS: usize = 18;

/// A section of a row: its silkscreen label and its parameters, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub label: &'static str,
    pub params: Vec<usize>,
}

/// A row of sections side by side.
pub type Row = Vec<Section>;

/// A page: the key that shows it (empty when there is one page), its rows, and the words its
/// key already says (left off the knobs' labels).
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub key: String,
    pub rows: Vec<Row>,
    pub prefix: String,
}

/// The model code on the faceplate, as in the program.
pub fn model_code(id: &str) -> &'static str {
    match id {
        macro_osc::ID => "MO-24",
        kick::ID => "KK-1",
        bass_line::ID => "BL-302",
        triple_osc::ID => "TO-3",
        pluck::ID => "PL-1",
        wave_draw::ID => "WD-200",
        engine_sound::ID => "EN-8",
        drum_synth::ID => "DS-8",
        monstro::ID => "MN-3",
        resonator::ID => "RS-6",
        digital_osc::ID => "DO-47",
        fm6::ID => "FM-6",
        chip_opl::ID => "CH-OPL",
        chip_opn::ID => "CH-OPN",
        chip_opll::ID => "CH-OPLL",
        chip_sid::ID => "CH-6581",
        chip_psg::ID => "CH-PSG",
        lemon_limiter::ID => "LX-18",
        press::ID => "MX-4",
        lemon_transient_processor::ID => "TP-2",
        lemon_soft_clipper::ID => "SC-1",
        lemon_fast_distortion::ID => "FD-1",
        prowave_engine::tune::ID => "PT-1",
        prowave_engine::texture::ID => "TX-1",
        circuit::ID => "CO-1",
        resonator_fx::ID => "RS-6F",
        spectral::ID => "SP-1",
        _ => "PW-1",
    }
}

fn sec(label: &'static str, params: &[usize]) -> Section {
    Section {
        label,
        params: params.to_vec(),
    }
}

/// One row per six parameters of `params`, choices first (an instrument's panel).
fn rows_of(info: &NativeFxInfo, params: &[usize]) -> Vec<Row> {
    let (choices, knobs): (Vec<usize>, Vec<usize>) = params
        .iter()
        .filter(|k| **k < info.params.len())
        .partition(|k| !info.params[**k].choices.is_empty());
    choices
        .chunks(6)
        .chain(knobs.chunks(6))
        .map(|c| vec![sec("", c)])
        .collect()
}

/// An instrument's pages: (key, parameters, prefix).
fn instrument_pages(info: &NativeFxInfo) -> Vec<(String, Vec<usize>, String)> {
    let page =
        |key: &str, params: Vec<usize>, prefix: &str| (key.to_string(), params, prefix.to_string());
    match info.id {
        fm6::ID => {
            let mut out: Vec<_> = (1..=6)
                .map(|n| {
                    page(
                        &format!("OP {n}"),
                        ((6 - n) * fm6::OP_PARAMS..(7 - n) * fm6::OP_PARAMS).collect(),
                        &format!("Op {n} "),
                    )
                })
                .collect();
            out.push(page(
                "PITCH",
                (fm6::PITCH_ENV..fm6::PITCH_ENV + 8).collect(),
                "Pitch ",
            ));
            out.push(page("LFO", (137..fm6::TRANSPOSE).collect(), "LFO "));
            out.push(page(
                "VOICE",
                vec![
                    fm6::ALGORITHM,
                    fm6::FEEDBACK,
                    136,
                    fm6::TRANSPOSE,
                    fm6::VOICES,
                    fm6::LEVEL,
                ],
                "",
            ));
            out
        }
        // The points are drawn on the display, not turned.
        wave_draw::ID => vec![page("", (0..wave_draw::WAVE).collect(), "")],
        triple_osc::ID => {
            use triple_osc as to;
            let mut out: Vec<_> = (1..=3)
                .map(|n| {
                    page(
                        &format!("OSC {n}"),
                        ((n - 1) * to::OSC_PARAMS..n * to::OSC_PARAMS).collect(),
                        &format!("Osc {n} "),
                    )
                })
                .collect();
            out.push(page("VOICE", (to::MOD_1..to::COUNT).collect(), ""));
            out
        }
        monstro::ID => {
            use monstro as mo;
            let m = |t: usize, n: usize| mo::MATRIX + t * 4..mo::MATRIX + (t + n) * 4;
            let mut osc3: Vec<usize> = (mo::O3_VOL..mo::LFO).collect();
            osc3.push(mo::O23_MOD);
            let mut vol: Vec<usize> = m(mo::T_VOL, 3).collect();
            vol.push(mo::LEVEL);
            vec![
                page("OSC 1", (mo::O1_VOL..mo::O2_VOL).collect(), "Osc 1 "),
                page("OSC 2", (mo::O2_VOL..mo::O3_VOL).collect(), "Osc 2 "),
                page("OSC 3", osc3, "Osc 3 "),
                page("LFO", (mo::LFO..mo::ENV).collect(), ""),
                page("ENV", (mo::ENV..mo::O23_MOD).collect(), ""),
                page("VOL", vol, "Vol "),
                page("PHASE", m(mo::T_PHS, 3).collect(), "Phase "),
                page("PITCH", m(mo::T_PIT, 3).collect(), "Pitch "),
                page("PW SUB", m(mo::T_PW, 2).collect(), ""),
            ]
        }
        drum_synth::ID => {
            use drum_synth as ds;
            let mut out: Vec<_> = (1..=ds::PADS)
                .map(|n| {
                    page(
                        &format!("PAD {n}"),
                        (ds::PAD + (n - 1) * ds::PAD_PARAMS..ds::PAD + n * ds::PAD_PARAMS)
                            .collect(),
                        &format!("Pad {n} "),
                    )
                })
                .collect();
            out.push(page("KIT", vec![ds::VOLUME, ds::OVERSAMPLING], ""));
            out
        }
        chip_opn::ID => {
            use chip_opn as opn;
            let mut out: Vec<_> = (1..=4)
                .map(|n| {
                    page(
                        &format!("OP {n}"),
                        (opn::OPS + (n - 1) * opn::OP_PARAMS..opn::OPS + n * opn::OP_PARAMS)
                            .collect(),
                        &format!("Op {n} "),
                    )
                })
                .collect();
            out.push(page(
                "VOICE",
                vec![
                    opn::ALGORITHM,
                    opn::FEEDBACK,
                    opn::TUNE,
                    opn::VOICES,
                    opn::LEVEL,
                ],
                "",
            ));
            out
        }
        _ if info.params.len() > PAGE_PARAMS * 2 => {
            let all: Vec<usize> = (0..info.params.len()).collect();
            all.chunks(PAGE_PARAMS)
                .enumerate()
                .map(|(n, c)| page(&(n + 1).to_string(), c.to_vec(), ""))
                .collect()
        }
        _ => vec![page("", (0..info.params.len()).collect(), "")],
    }
}

/// An effect's rows (Press: one page per band, as the program's band keys show one band).
fn effect_pages(info: &NativeFxInfo) -> Vec<Page> {
    use lemon_fast_distortion as fd;
    use lemon_transient_processor as tp;
    use press::params as pp;
    let one = |rows: Vec<Row>| {
        vec![Page {
            key: String::new(),
            rows,
            prefix: String::new(),
        }]
    };
    match info.id {
        lemon_limiter::ID => one(vec![
            vec![
                sec("Gain", &[0, 1]),
                sec("Noise gate", &[15, 16, 17]),
                sec("Safety", &[18]),
            ],
            vec![sec("Limiter", &[2, 3, 4, 5, 6, 7])],
            vec![sec("Compressor", &[8, 9, 10, 11, 12, 13, 14])],
        ]),
        press::ID => ["LOW", "MID", "HIGH", "MASTER"]
            .iter()
            .enumerate()
            .map(|(band, key)| {
                let b = 7 + band * 13;
                let band_params: Vec<usize> = (b..b + 13).collect();
                Page {
                    key: key.to_string(),
                    rows: vec![
                        vec![sec(
                            "Crossover",
                            &[
                                pp::P_LOW_FREQ,
                                pp::P_LOW_SLOPE,
                                pp::P_HIGH_FREQ,
                                pp::P_HIGH_SLOPE,
                                pp::P_LMH_DELAY,
                                pp::P_LMH_MIX,
                                pp::P_LOW_CUT,
                            ],
                        )],
                        vec![Section {
                            label: ["Low band", "Mid band", "High band", "Master"][band],
                            params: band_params[..7].to_vec(),
                        }],
                        vec![sec("", &band_params[7..])],
                        vec![sec(
                            "Options",
                            &[pp::OVERSAMPLING, pp::FIR, pp::MID_TRIGGER, pp::SAFETY],
                        )],
                    ],
                    prefix: String::new(),
                }
            })
            .collect(),
        tp::ID => one(vec![
            vec![
                sec("Attack", &[tp::ATTACK, tp::ATTACK_SHAPE]),
                sec("Release", &[tp::RELEASE, tp::RELEASE_SHAPE]),
                sec("Output", &[tp::DRIVE, tp::GAIN]),
            ],
            vec![
                sec("Split", &[tp::SPLIT, tp::BALANCE]),
                sec("Processing", &[tp::ON]),
            ],
        ]),
        lemon_soft_clipper::ID => one(vec![vec![sec("", &[0, 1])]]),
        fd::ID => one(vec![vec![sec(
            "",
            &[fd::PRE, fd::THRESHOLD, fd::MIX, fd::POST, fd::TYPE],
        )]]),
        _ => {
            // Knobs six to a row, then the choices.
            let knobs: Vec<usize> = (0..info.params.len())
                .filter(|k| info.params[*k].choices.is_empty())
                .collect();
            let choices: Vec<usize> = (0..info.params.len())
                .filter(|k| !info.params[*k].choices.is_empty())
                .collect();
            one(knobs
                .chunks(6)
                .chain(choices.chunks(6))
                .map(|c| vec![sec("", c)])
                .collect())
        }
    }
}

/// The pages of the instrument or effect `info`.
pub fn pages(info: &NativeFxInfo, instrument: bool) -> Vec<Page> {
    if instrument {
        instrument_pages(info)
            .into_iter()
            .map(|(key, params, prefix)| Page {
                rows: rows_of(info, &params),
                key,
                prefix,
            })
            .collect()
    } else {
        effect_pages(info)
    }
}

/// The label of parameter `k` on `page`: an instrument's without its page's words, an effect's
/// without its section's word ("Limiter attack time" in "Limiter": "Attack").
pub fn label(info: &NativeFxInfo, instrument: bool, page: &Page, k: usize) -> String {
    let name = info.params.get(k).map_or("", |p| p.name);
    if instrument {
        return name
            .strip_prefix(page.prefix.as_str())
            .unwrap_or(name)
            .to_string();
    }
    let mut words: Vec<&str> = name.split_whitespace().collect();
    if words.len() > 1
        && matches!(
            words[0],
            "Limiter" | "Comp" | "Noise" | "LOW" | "MID" | "HIGH" | "MASTER" | "LMH"
        )
    {
        words.remove(0);
    }
    if words.len() > 1 && words.last() == Some(&"time") {
        words.pop();
    }
    let s = words.join(" ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

/// The host's group of parameter `k`: the key of the first page it is on ("WAVE" for Wave
/// Draw's points, empty when the plug-in has one page).
pub fn group(pages: &[Page], k: usize) -> String {
    for p in pages {
        if p.rows.iter().flatten().any(|s| s.params.contains(&k)) {
            return p.key.clone();
        }
    }
    "WAVE".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every parameter of every plug-in is on a page (Wave Draw's points on its display).
    #[test]
    fn every_parameter_has_a_place() {
        let all = prowave_engine::native_instruments()
            .iter()
            .map(|i| (i, true))
            .chain(prowave_engine::native_effects().iter().map(|i| (i, false)));
        for (info, instrument) in all {
            let pages = pages(info, instrument);
            assert!(!pages.is_empty());
            for k in 0..info.params.len() {
                let placed = pages
                    .iter()
                    .any(|p| p.rows.iter().flatten().any(|s| s.params.contains(&k)));
                let drawn = info.id == wave_draw::ID && k >= wave_draw::WAVE;
                assert!(placed || drawn, "{} parameter {k} has no place", info.id);
            }
        }
    }
}
