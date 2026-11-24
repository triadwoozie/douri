//! Methods for a track's MIDI FX (Track > MIDI FX…): the chain itself, and every setting of
//! the Arpeggiator by name.
use super::params::P;
use super::protocol::RpcError;
use super::registry::{m, p, Method, Reply, Ty};
use super::state;
use super::{ok, Cx};
use crate::arp_patterns::{randomize, PATTERNS};
use crate::midi_fx::{FxSlot, MidiFx, MAX_FX};
use prowave_engine::{ArpMode, ArpRate, ArpSettings, ARP_STEPS};
use serde_json::{json, Value};

/// The kinds `midi_fx.add` takes, as the project file names them.
const KINDS: [&str; 11] = [
    "scripter",
    "modulator",
    "transposer",
    "velocity",
    "scale",
    "chord",
    "repeater",
    "arpeggiator",
    "randomizer",
    "swing",
    "euclid",
];
const RATES: [&str; 12] = [
    "1/4.", "1/4", "1/4T", "1/8.", "1/8", "1/8T", "1/16.", "1/16", "1/16T", "1/32.", "1/32",
    "1/32T",
];
const MODES: [&str; 7] = [
    "up",
    "down",
    "up_down",
    "down_up",
    "random",
    "as_played",
    "chord",
];
const TRIGGERS: [&str; 2] = ["restart", "song_position"];
/// The names of `arp_patterns::PATTERNS`.
const PRESETS: [&str; 10] = [
    "Straight",
    "Accents",
    "Offbeat",
    "Gallop",
    "Three Three Two",
    "Long Short",
    "Octave Jump",
    "Fifths",
    "Stabs",
    "Ramp",
];

/// How an effect is named here: the Arpeggiator by its plain name.
fn kind_name(fx: &MidiFx) -> &'static str {
    match fx {
        MidiFx::Arpeggiator(_) => "arpeggiator",
        other => other.name(),
    }
}

fn arp_json(a: &ArpSettings) -> Value {
    let steps: Vec<Value> = a
        .steps
        .iter()
        .take(usize::from(a.length))
        .enumerate()
        .map(|(i, s)| {
            json!({
                "step": i + 1,
                "on": s.on,
                "velocity": s.vel,
                "tie": s.tie,
                "chord": s.chord,
                "transpose": s.transpose,
            })
        })
        .collect();
    json!({
        "rate": a.rate.name(),
        "mode": MODES[usize::from(a.mode.index())],
        "octaves": a.octaves,
        "gate": a.gate,
        "swing": a.swing,
        "step_velocity": a.step_velocity,
        "latch": a.latch,
        "trigger": TRIGGERS[usize::from(a.sync)],
        "length": a.length,
        "steps": steps,
    })
}

fn slot_json(i: usize, slot: &FxSlot) -> Value {
    let mut o = json!({
        "index": i + 1,
        "kind": kind_name(&slot.fx),
        "on": slot.on,
        "settings": slot.fx.numbers(),
    });
    if let (MidiFx::Arpeggiator(a), Some(map)) = (&slot.fx, o.as_object_mut()) {
        map.insert("arpeggiator".into(), arp_json(a));
    }
    o
}

fn chain_json(cx: &Cx, ti: usize) -> Value {
    let effects: Vec<Value> = cx.app.project.tracks[ti]
        .midi_fx
        .iter()
        .enumerate()
        .map(|(i, s)| slot_json(i, s))
        .collect();
    json!({
        "track": state::strip_id(&cx.core.ids, ti),
        "effects": effects,
    })
}

/// The effect `index` (from 1) of track `ti`.
fn req_index(cx: &Cx, ti: usize, params: &P) -> Result<usize, RpcError> {
    let count = cx.app.project.tracks[ti].midi_fx.len();
    let Some(i) = params.i64("index") else {
        return Err(RpcError::invalid(
            "index",
            "`index` is needed: the effect's place in the chain, from 1",
        ));
    };
    if i < 1 || i as usize > count {
        return Err(RpcError::not_found(format!(
            "the track has {count} MIDI FX; there is no number {i}"
        )));
    }
    Ok(i as usize - 1)
}

fn list(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let ti = state::req_track(&cx.app.project, &cx.core.ids, params, "track")?;
    ok(chain_json(cx, ti))
}

fn add(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let ti = state::req_track(&cx.app.project, &cx.core.ids, params, "track")?;
    let kind = params.req_str("kind")?;
    let Some(fx) = MidiFx::all_defaults()
        .into_iter()
        .find(|fx| kind_name(fx) == kind)
    else {
        return Err(RpcError::invalid(
            "kind",
            format!("there is no MIDI FX \"{kind}\""),
        ));
    };
    let chain = &mut cx.app.project.tracks[ti].midi_fx;
    if chain.len() >= MAX_FX {
        return Err(RpcError::failed(format!(
            "a track has {MAX_FX} MIDI FX at most"
        )));
    }
    chain.push(FxSlot {
        fx,
        on: params.bool("on").unwrap_or(true),
    });
    cx.app.project.song_rev += 1;
    ok(chain_json(cx, ti))
}

fn remove(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let ti = state::req_track(&cx.app.project, &cx.core.ids, params, "track")?;
    let i = req_index(cx, ti, params)?;
    cx.app.project.tracks[ti].midi_fx.remove(i);
    cx.app.project.song_rev += 1;
    ok(chain_json(cx, ti))
}

fn set(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let ti = state::req_track(&cx.app.project, &cx.core.ids, params, "track")?;
    let i = req_index(cx, ti, params)?;
    let slot = &mut cx.app.project.tracks[ti].midi_fx[i];
    if let Some(on) = params.bool("on") {
        slot.on = on;
    }
    if let Some(list) = params.arr("settings") {
        let mut numbers = Vec::with_capacity(list.len());
        for v in list {
            let Some(x) = v.as_f64() else {
                return Err(RpcError::invalid(
                    "settings",
                    "`settings` is a list of numbers",
                ));
            };
            numbers.push(x as f32);
        }
        let script = match &slot.fx {
            MidiFx::Scripter { source } => source.clone(),
            _ => String::new(),
        };
        if let Some(fx) = MidiFx::from_parts(slot.fx.name(), &numbers, &script) {
            slot.fx = fx;
        }
    }
    cx.app.project.song_rev += 1;
    ok(slot_json(i, &cx.app.project.tracks[ti].midi_fx[i]))
}

/// A whole number of `o` under `key`, inside `lo..=hi`.
fn step_int(o: &Value, key: &str, lo: i64, hi: i64) -> Result<Option<i64>, RpcError> {
    match o.get(key) {
        None => Ok(None),
        Some(v) => match v.as_i64().filter(|x| (lo..=hi).contains(x)) {
            Some(x) => Ok(Some(x)),
            None => Err(RpcError::invalid(
                "steps",
                format!("a step's `{key}` is a whole number from {lo} to {hi}; got {v}"),
            )),
        },
    }
}

fn step_flag(o: &Value, key: &str) -> Result<Option<bool>, RpcError> {
    match o.get(key) {
        None => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| {
            RpcError::invalid(
                "steps",
                format!("a step's `{key}` is true or false; got {v}"),
            )
        }),
    }
}

fn arpeggiator(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let ti = state::req_track(&cx.app.project, &cx.core.ids, params, "track")?;
    // The Arpeggiator named by `index`, or the track's first; a track without one gets one.
    let at = if params.has("index") {
        let i = req_index(cx, ti, params)?;
        if !matches!(
            cx.app.project.tracks[ti].midi_fx[i].fx,
            MidiFx::Arpeggiator(_)
        ) {
            return Err(RpcError::invalid(
                "index",
                format!("MIDI FX {} of the track is not an Arpeggiator", i + 1),
            ));
        }
        Some(i)
    } else {
        cx.app.project.tracks[ti]
            .midi_fx
            .iter()
            .position(|s| matches!(s.fx, MidiFx::Arpeggiator(_)))
    };
    let mut arp = match at {
        Some(i) => match cx.app.project.tracks[ti].midi_fx[i].fx {
            MidiFx::Arpeggiator(a) => a,
            _ => ArpSettings::default(),
        },
        None => ArpSettings::default(),
    };
    let was = arp;
    let pick = |key: &str, names: &[&str]| -> Option<u8> {
        params
            .str(key)
            .and_then(|v| names.iter().position(|n| *n == v))
            .map(|i| i as u8)
    };
    if let Some(i) = pick("rate", &RATES) {
        arp.rate = ArpRate::from_index(i);
    }
    if let Some(i) = pick("mode", &MODES) {
        arp.mode = ArpMode::from_index(i);
    }
    if let Some(i) = pick("trigger", &TRIGGERS) {
        arp.sync = i == 1;
    }
    if let Some(v) = params.i64("octaves") {
        arp.octaves = v as u8;
    }
    if let Some(v) = params.i64("gate") {
        arp.gate = v as u8;
    }
    if let Some(v) = params.i64("swing") {
        arp.swing = v as u8;
    }
    if let Some(v) = params.i64("step_velocity") {
        arp.step_velocity = v as u8;
    }
    if let Some(v) = params.bool("latch") {
        arp.latch = v;
    }
    // A preset brings its own length; a length given with it is set after it.
    if let Some(i) = pick("preset", &PRESETS) {
        PATTERNS[usize::from(i)].apply(&mut arp);
    }
    if let Some(v) = params.i64("length") {
        arp.length = v as u8;
    }
    if params.bool("randomize") == Some(true) {
        let seed = arp.seed.wrapping_add(1) & 0x00ff_ffff;
        arp.seed = seed;
        randomize(&mut arp, seed);
    }
    if let Some(steps) = params.arr("steps") {
        for o in steps {
            let Some(n) = step_int(o, "step", 1, ARP_STEPS as i64)? else {
                return Err(RpcError::invalid(
                    "steps",
                    "each step names its place: {\"step\": 1, ...}",
                ));
            };
            let s = &mut arp.steps[n as usize - 1];
            if let Some(v) = step_flag(o, "on")? {
                s.on = v;
            }
            if let Some(v) = step_int(o, "velocity", 1, 127)? {
                s.vel = v as u8;
            }
            if let Some(v) = step_flag(o, "tie")? {
                s.tie = v;
            }
            if let Some(v) = step_flag(o, "chord")? {
                s.chord = v;
            }
            if let Some(v) = step_int(o, "transpose", -24, 24)? {
                s.transpose = v as i8;
            }
        }
    }
    let arp = arp.sanitized();
    let chain = &mut cx.app.project.tracks[ti].midi_fx;
    let i = match at {
        Some(i) => {
            chain[i].fx = MidiFx::Arpeggiator(arp);
            i
        }
        None => {
            if chain.len() >= MAX_FX {
                return Err(RpcError::failed(format!(
                    "a track has {MAX_FX} MIDI FX at most"
                )));
            }
            chain.push(FxSlot {
                fx: MidiFx::Arpeggiator(arp),
                on: true,
            });
            chain.len() - 1
        }
    };
    if let Some(on) = params.bool("on") {
        chain[i].on = on;
    }
    if at.is_none() || arp != was || params.has("on") {
        cx.app.project.song_rev += 1;
    }
    ok(slot_json(i, &cx.app.project.tracks[ti].midi_fx[i]))
}

pub fn methods() -> Vec<Method> {
    let track = || {
        p(
            "track",
            Ty::Any,
            "the track: its id (\"t3\"), its name, or its number in the list (from 1)",
        )
        .req()
    };
    let index = || {
        p(
            "index",
            Ty::Int,
            "the effect's place in the track's MIDI FX, from 1",
        )
    };
    vec![
        m("midi_fx.list", "The MIDI FX of a track (Track > MIDI FX…), in the order the notes go through them.",
            vec![track()],
            "{track, effects: [{index, kind, on, settings: [numbers], arpeggiator?: {rate, mode, octaves, gate, swing, step_velocity, latch, trigger, length, steps}}]}", list),
        m("midi_fx.add", "Adds a MIDI FX at the end of a track's chain, with its start settings.",
            vec![
                track(),
                p("kind", Ty::Str, "which effect").req().vals(&KINDS),
                p("on", Ty::Bool, "false adds it switched off (default true)"),
            ],
            "the chain, as midi_fx.list gives it", add).changes(),
        m("midi_fx.remove", "Takes a MIDI FX out of a track's chain.",
            vec![track(), index().req()],
            "the chain, as midi_fx.list gives it", remove).changes(),
        m("midi_fx.set", "Switches a MIDI FX on or off, or sets its settings as the numbers midi_fx.list gives (the Arpeggiator's are easier by name: midi_fx.arpeggiator).",
            vec![
                track(),
                index().req(),
                p("on", Ty::Bool, "on or off"),
                p("settings", Ty::Arr, "the effect's settings as numbers, in the order midi_fx.list gives them; missing ones go to their start values"),
            ],
            "the effect, as in midi_fx.list", set).changes(),
        m("midi_fx.arpeggiator", "Sets the Arpeggiator of a track (the first one in its MIDI FX, or the one at `index`; a track without one gets one). It plays the keys held on the track, live or from its regions, through a pattern of up to 32 steps. Only what is named changes.",
            vec![
                track(),
                index(),
                p("on", Ty::Bool, "the effect on or off"),
                p("rate", Ty::Str, "the length of a step (\".\" dotted, \"T\" triplet)").vals(&RATES),
                p("mode", Ty::Str, "the order the held keys are played in; chord plays them all on every step").vals(&MODES),
                p("octaves", Ty::Int, "how many octaves the keys are played over").range(1.0, 4.0),
                p("gate", Ty::Int, "how long a note lasts, of a step").unit("percent").range(5.0, 200.0),
                p("swing", Ty::Int, "how late every second step is: 100 is half a step, 67 a triplet feel").unit("percent").range(0.0, 100.0),
                p("step_velocity", Ty::Int, "how much of a note's velocity comes from its step (0: all from the key played)").unit("percent").range(0.0, 100.0),
                p("latch", Ty::Bool, "keys stay held after they are let go, until new ones are pressed"),
                p("trigger", Ty::Str, "restart: each new chord starts the pattern again; song_position: the steps stay on the song's grid").vals(&TRIGGERS),
                p("length", Ty::Int, "the steps played before the pattern starts again").range(1.0, 32.0),
                p("preset", Ty::Str, "a built-in step pattern (it sets the steps and the length)").vals(&PRESETS),
                p("randomize", Ty::Bool, "true: new random steps for the length"),
                p("steps", Ty::Arr, "steps to change: [{\"step\": 1, \"on\": true, \"velocity\": 100, \"tie\": false, \"chord\": false, \"transpose\": 0}, ...]; step is its number from 1, tie holds the note through the next step, chord plays all the held keys, transpose is semitones from -24 to 24; what is left out stays"),
            ],
            "the effect: {index, kind, on, settings, arpeggiator: {rate, mode, octaves, gate, swing, step_velocity, latch, trigger, length, steps: [{step, on, velocity, tie, chord, transpose}]}}", arpeggiator).changes(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_here_are_the_ones_the_arpeggiator_has() {
        assert_eq!(
            RATES.to_vec(),
            ArpRate::ALL.iter().map(|r| r.name()).collect::<Vec<_>>()
        );
        assert_eq!(MODES.len(), ArpMode::ALL.len());
        assert_eq!(
            PRESETS.to_vec(),
            PATTERNS.iter().map(|p| p.name).collect::<Vec<_>>()
        );
        let kinds: Vec<&str> = MidiFx::all_defaults().iter().map(kind_name).collect();
        assert_eq!(KINDS.to_vec(), kinds);
    }
}
