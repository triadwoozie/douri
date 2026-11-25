// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! The plug-in this build wraps (`PROWAVE_PLUG_ID`, the Macro Oscillator by default) as a host
//! runs it: through nih-plug's CLAP wrapper, at 44.1 and 96 kHz, in blocks of odd sizes. In
//! this (debug) build the wrapper runs every `process` call inside the allocation checker,
//! which counts any allocation on the audio thread.

#[path = "common/clap_host.rs"]
mod clap_host;

use clap_host::{instrument_score, peak, render, rms, test_signal, Ev, Host, Score};
use prowave_plug::selected;

fn host(rate: f64, max_block: u32) -> Host {
    Host::new(
        &prowave_plug::clap_entry,
        c"/tmp/prowave-plug.clap",
        rate,
        max_block,
    )
}

/// The plug-in sounds (a chord for an instrument, a test signal through an effect) with every
/// parameter moved while it plays, its output is finite, and nothing allocates on the audio
/// thread.
#[test]
fn it_plays_without_allocating() {
    let info = prowave_plug::selected_info();
    for rate in [44_100.0, 96_000.0] {
        assert_no_alloc::reset_violation_count();
        let mut h = host(rate, 4096);
        assert_eq!(h.plugin_id, selected::CLAP_ID);
        assert_eq!(h.plugin_name, selected::NAME);
        assert_eq!(h.inputs > 0, !selected::INSTRUMENT);
        let mut score: Score = if selected::INSTRUMENT {
            instrument_score()
        } else {
            Vec::new()
        };
        // Every parameter to its top, its bottom and back to its default (CLAP gives plain
        // values).
        for (k, p) in h.params.clone().iter().enumerate() {
            score.push((1.3, Ev::Param(k, p.max)));
            score.push((1.4, Ev::Param(k, p.min)));
            score.push((1.5, Ev::Param(k, p.default)));
        }
        let (l, r) = render(
            &mut h,
            &score,
            2.0,
            &[512, 333, 4096, 64, 1],
            &mut test_signal(rate),
        );
        assert!(
            l.iter().chain(&r).all(|v| v.is_finite()),
            "finite at {rate}"
        );
        let first = (1.2 * rate) as usize;
        let loud = rms(&l[..first]).max(rms(&r[..first]));
        assert!(loud > 1e-3, "{}: rms {loud} at {rate}", info.id);
        assert!(peak(&l).max(peak(&r)) < 100.0);
        drop(h);
        assert_eq!(
            assert_no_alloc::violation_count(),
            0,
            "allocations on the audio thread at {rate}"
        );
    }
}

/// The host sees the registry's parameters with their names, ranges and value texts, and the
/// values come back from a saved state.
#[test]
fn parameters_and_state() {
    let info = prowave_plug::selected_info();
    let h = host(48_000.0, 512);
    assert_eq!(h.params.len(), info.params.len());
    for (k, (got, want)) in h.params.iter().zip(info.params).enumerate() {
        assert_eq!(got.name, want.name);
        // nih-plug gives CLAP a whole-number parameter as its step, 0 to the number of steps,
        // and the values come back through its normalised form within a hair.
        let near = |a: f64, b: f32| (a - f64::from(b)).abs() < 1e-3;
        assert!(
            near(got.max - got.min, want.max - want.min),
            "{} range",
            want.name
        );
        assert!(
            near(got.default - got.min, want.default - want.min),
            "{} default",
            want.name
        );
        assert_eq!(
            h.value_text(k, got.default),
            prowave_plug::params::value_text(info, k, want.default as i32)
        );
    }
    // Change the last parameter, save, load into a new instance.
    let k = h.params.len() - 1;
    let v = if h.params[k].default == h.params[k].max {
        h.params[k].min
    } else {
        h.params[k].max
    };
    let mut h = h;
    let mut l = vec![0.0f32; 64];
    let mut r = vec![0.0f32; 64];
    h.process(&mut l, &mut r, &[(0, Ev::Param(k, v))]);
    assert_eq!(h.param_value(k).round(), v.round());
    let state = h.save_state();
    drop(h);
    let h2 = host(48_000.0, 512);
    h2.load_state(&state);
    assert_eq!(h2.param_value(k).round(), v.round());
}

/// The latency the host is told is the engine's.
#[test]
fn latency_is_reported() {
    let info = prowave_plug::selected_info();
    let h = host(48_000.0, 512);
    let core = prowave_plug::core::PlugCore::open(info, selected::INSTRUMENT, 48_000, 512, &[])
        .expect("opens");
    assert_eq!(h.latency(), core.latency_samples());
}

/// The registry's presets reach a CLAP host: a preset-discovery factory lists them (in the
/// plug-in's own location, as factory content, for this plug-in), and loading one through
/// `clap.preset-load` sets the parameters it names and keeps the others. clap-wrapper reads the
/// same factory for the Audio Unit's factory presets.
#[test]
fn factory_presets_reach_the_host() {
    use clap_sys::factory::preset_discovery::{
        CLAP_PRESET_DISCOVERY_IS_FACTORY_CONTENT, CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN,
    };
    let info = prowave_plug::selected_info();
    let found = clap_host::discover_presets(&prowave_plug::clap_entry, c"/tmp/prowave-plug.clap");
    assert_eq!(found.len(), info.presets.len(), "{}", info.id);
    for (i, (f, want)) in found.iter().zip(info.presets).enumerate() {
        assert_eq!(f.name, want.name);
        assert_eq!(f.load_key, i.to_string());
        assert_eq!(f.plugin_ids, vec![selected::CLAP_ID.to_string()]);
        assert_eq!(f.flags, CLAP_PRESET_DISCOVERY_IS_FACTORY_CONTENT);
        assert_eq!(f.location_kind, CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN);
    }
    let h = host(48_000.0, 512);
    if info.presets.is_empty() {
        assert_eq!(h.load_preset("0"), None, "no preset-load without presets");
        return;
    }
    for (i, preset) in info.presets.iter().enumerate() {
        let before: Vec<f64> = (0..info.params.len()).map(|k| h.param_value(k)).collect();
        assert_eq!(h.load_preset(&i.to_string()), Some(true), "{}", preset.name);
        for (k, was) in before.iter().enumerate() {
            let set = preset.values.iter().find(|(pk, _)| *pk == k);
            let now = h.param_value(k).round();
            match set {
                // nih-plug gives CLAP a whole-number parameter as steps from its minimum.
                Some((_, v)) => assert_eq!(
                    now,
                    f64::from(*v - info.params[k].min).round(),
                    "{} p{k}",
                    preset.name
                ),
                None => assert_eq!(now, was.round(), "{} kept p{k}", preset.name),
            }
        }
    }
    assert_eq!(h.load_preset(&info.presets.len().to_string()), Some(false));
}

/// An effect that takes notes (Tune) has a note input; the others and the instruments have the
/// inputs they had.
#[test]
fn note_input() {
    let h = host(48_000.0, 512);
    let want = u32::from(selected::INSTRUMENT || selected::TAKES_NOTES);
    assert_eq!(h.note_inputs(), want);
}
