//! The project as JSON, and finding the track, region or effect a request names.
use super::ids::Ids;
use super::params::{place_json, round3, round6, P};
use super::protocol::RpcError;
use crate::mixer::MASTER_STRIP;
use crate::model::{Project, Region, Track, TrackKind};
use crate::plugins::PluginSlot;
use crate::App;
use serde_json::{json, Map, Value};

pub fn kind_name(kind: TrackKind) -> &'static str {
    match kind {
        TrackKind::Instrument => "instrument",
        TrackKind::Audio => "audio",
        TrackKind::Drummer => "drummer",
        TrackKind::Bus => "bus",
        TrackKind::Output => "output",
        TrackKind::Aux => "aux",
        TrackKind::Folder => "folder",
        TrackKind::Vca => "vca",
        TrackKind::ExternalMidi => "external_midi",
    }
}

pub fn kind_from(name: &str) -> Option<TrackKind> {
    Some(match name {
        "instrument" => TrackKind::Instrument,
        "audio" => TrackKind::Audio,
        "drummer" => TrackKind::Drummer,
        "aux" => TrackKind::Aux,
        "vca" => TrackKind::Vca,
        "external_midi" => TrackKind::ExternalMidi,
        "folder" => TrackKind::Folder,
        _ => return None,
    })
}

fn color_hex(c: egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

// ------------------------------------------------------------------ finding things

/// The track a request names: its id ("t3"), its name, or its number in the track list
/// (1-based, as the mixer numbers the strips).
pub fn track_ref(p: &Project, ids: &Ids, v: &Value, key: &str) -> Result<usize, RpcError> {
    let listing = || {
        p.tracks
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{} \"{}\"", ids.track_id(i), t.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(n) = v.as_f64() {
        let i = n as i64;
        if n.fract() == 0.0 && i >= 1 && (i as usize) <= p.tracks.len() {
            return Ok(i as usize - 1);
        }
        return Err(RpcError::not_found(format!(
            "`{key}`: there is no track number {n}; the project has {} tracks (numbered from 1): {}",
            p.tracks.len(),
            listing()
        )));
    }
    let Some(text) = v.as_str() else {
        return Err(RpcError::invalid(
            key,
            format!("`{key}` must be a track id (\"t3\"), a track name or a track number"),
        ));
    };
    if let Some(i) = ids.track_index(text).filter(|i| *i < p.tracks.len()) {
        return Ok(i);
    }
    let exact: Vec<usize> = (0..p.tracks.len())
        .filter(|i| p.tracks[*i].name == text)
        .collect();
    let found = if exact.is_empty() {
        (0..p.tracks.len())
            .filter(|i| p.tracks[*i].name.eq_ignore_ascii_case(text))
            .collect()
    } else {
        exact
    };
    match found.as_slice() {
        [i] => Ok(*i),
        [] => Err(RpcError::not_found(format!(
            "`{key}`: there is no track \"{text}\"; the tracks are: {}",
            listing()
        ))),
        many => Err(RpcError::invalid(
            key,
            format!(
                "`{key}`: {} tracks are named \"{text}\"; use an id: {}",
                many.len(),
                many.iter()
                    .map(|i| ids.track_id(*i))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// The track in parameter `key`.
pub fn req_track(p: &Project, ids: &Ids, params: &P, key: &str) -> Result<usize, RpcError> {
    match params.get(key) {
        Some(v) => track_ref(p, ids, v, key),
        None => Err(RpcError::invalid(
            key,
            format!("`{key}` is needed: a track id (\"t3\"), a track name or a track number"),
        )),
    }
}

/// A channel strip: a track, or the Stereo Out ("master").
pub fn strip_ref(p: &Project, ids: &Ids, v: &Value, key: &str) -> Result<usize, RpcError> {
    if let Some(text) = v.as_str() {
        let low = text.to_ascii_lowercase();
        if low == "master" || low == "stereo out" || low == "stereo_out" {
            return Ok(MASTER_STRIP);
        }
    }
    track_ref(p, ids, v, key)
}

pub fn req_strip(p: &Project, ids: &Ids, params: &P, key: &str) -> Result<usize, RpcError> {
    match params.get(key) {
        Some(v) => strip_ref(p, ids, v, key),
        None => Err(RpcError::invalid(
            key,
            format!(
                "`{key}` is needed: a track id (\"t3\"), a track name, a track number, or \"master\" for the Stereo Out"
            ),
        )),
    }
}

pub fn strip_id(ids: &Ids, strip: usize) -> String {
    if strip == MASTER_STRIP {
        "master".into()
    } else {
        ids.track_id(strip)
    }
}

/// The region a request names: its id ("r12"), or its name when only one region has it.
pub fn region_ref(
    p: &Project,
    ids: &Ids,
    v: &Value,
    key: &str,
) -> Result<(usize, usize), RpcError> {
    let Some(text) = v.as_str() else {
        return Err(RpcError::invalid(
            key,
            format!("`{key}` must be a region id (\"r12\") or a region name"),
        ));
    };
    if let Some((t, r)) = ids.region_at(text) {
        if p.tracks.get(t).is_some_and(|tr| r < tr.regions.len()) {
            return Ok((t, r));
        }
    }
    let named: Vec<(usize, usize)> = p
        .tracks
        .iter()
        .enumerate()
        .flat_map(|(t, tr)| {
            tr.regions
                .iter()
                .enumerate()
                .filter(|(_, r)| r.name == text)
                .map(move |(r, _)| (t, r))
        })
        .collect();
    match named.as_slice() {
        [one] => Ok(*one),
        [] => Err(RpcError::not_found(format!(
            "`{key}`: there is no region \"{text}\" (state.get lists the regions with their ids)"
        ))),
        many => Err(RpcError::invalid(
            key,
            format!(
                "`{key}`: {} regions are named \"{text}\"; use an id: {}",
                many.len(),
                many.iter()
                    .map(|(t, r)| ids.region_id(*t, *r))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

pub fn req_region(
    p: &Project,
    ids: &Ids,
    params: &P,
    key: &str,
) -> Result<(usize, usize), RpcError> {
    match params.get(key) {
        Some(v) => region_ref(p, ids, v, key),
        None => Err(RpcError::invalid(
            key,
            format!("`{key}` is needed: a region id (\"r12\") or a region name"),
        )),
    }
}

/// An effect in a channel strip: a built-in one by its slot, or a plug-in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FxRef {
    /// `Project` insert `k` of the strip (0-based).
    Insert(usize),
    /// Plug-in effect `j` of the strip (0-based).
    Plugin(usize),
}

pub fn inserts_of(p: &Project, strip: usize) -> Option<&Vec<crate::model::Insert>> {
    if strip == MASTER_STRIP {
        Some(&p.master_inserts)
    } else {
        p.tracks.get(strip).map(|t| &t.inserts)
    }
}

/// The effect a request names on `strip`: "insert:2" (the second built-in effect),
/// "plugin:17" (the plug-in with that uid) or its name when only one effect has it.
pub fn fx_ref(p: &Project, strip: usize, v: &Value, key: &str) -> Result<FxRef, RpcError> {
    let inserts = inserts_of(p, strip).map_or(0, Vec::len);
    let plugins: &[PluginSlot] = p.strip_plugins(strip).map_or(&[], |v| v.as_slice());
    let listing = || {
        let mut all: Vec<String> = inserts_of(p, strip)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(k, i)| format!("insert:{} \"{}\"", k + 1, i.name()))
            .collect();
        all.extend(
            plugins
                .iter()
                .map(|s| format!("plugin:{} \"{}\"", s.uid, s.name)),
        );
        if all.is_empty() {
            "the strip has no effects".to_string()
        } else {
            format!("its effects are: {}", all.join(", "))
        }
    };
    let Some(text) = v.as_str() else {
        return Err(RpcError::invalid(
            key,
            format!(
                "`{key}` must be an effect: \"insert:1\", \"plugin:<uid>\" or its name; {}",
                listing()
            ),
        ));
    };
    if let Some(n) = text
        .strip_prefix("insert:")
        .and_then(|n| n.parse::<usize>().ok())
    {
        if n >= 1 && n <= inserts {
            return Ok(FxRef::Insert(n - 1));
        }
    } else if let Some(uid) = text
        .strip_prefix("plugin:")
        .and_then(|n| n.parse::<u64>().ok())
    {
        if let Some(j) = plugins.iter().position(|s| s.uid == uid) {
            return Ok(FxRef::Plugin(j));
        }
    } else {
        let mut found: Vec<FxRef> = inserts_of(p, strip)
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(_, i)| i.name().eq_ignore_ascii_case(text))
            .map(|(k, _)| FxRef::Insert(k))
            .collect();
        found.extend(
            plugins
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.name.eq_ignore_ascii_case(text)
                        || short_name(&s.name).eq_ignore_ascii_case(text)
                })
                .map(|(j, _)| FxRef::Plugin(j)),
        );
        match found.as_slice() {
            [one] => return Ok(*one),
            [] => {}
            _ => {
                return Err(RpcError::invalid(
                    key,
                    format!(
                        "`{key}`: several effects are named \"{text}\"; {}",
                        listing()
                    ),
                ))
            }
        }
    }
    Err(RpcError::not_found(format!(
        "`{key}`: the strip has no effect \"{text}\"; {}",
        listing()
    )))
}

/// "Maker: Name" without the maker.
pub fn short_name(name: &str) -> &str {
    name.split_once(": ").map_or(name, |(_, n)| n)
}

// ------------------------------------------------------------------ JSON

pub fn slot_json(slot: &PluginSlot) -> Value {
    let format = match crate::plugins::format_label(&slot.id) {
        "" => "built-in",
        f => f,
    };
    json!({
        "ref": format!("plugin:{}", slot.uid),
        "id": slot.id,
        "name": slot.name,
        "format": format,
        "bypass": slot.bypass,
    })
}

/// The effects of a strip, in the order the sound goes through them: the built-in ones, then
/// the plug-ins.
pub fn effects_json(p: &Project, strip: usize) -> Vec<Value> {
    let mut out = Vec::new();
    for (k, ins) in inserts_of(p, strip).into_iter().flatten().enumerate() {
        let mut values = Map::new();
        for (info, v) in ins.kind.params().iter().zip(ins.params.iter()) {
            values.insert(info.name.to_string(), json!(round6(f64::from(*v))));
        }
        out.push(json!({
            "ref": format!("insert:{}", k + 1),
            "id": format!("builtin:{}", ins.name()),
            "name": ins.name(),
            "format": "built-in",
            "bypass": ins.bypass,
            "values": values,
        }));
    }
    for slot in p.strip_plugins(strip).into_iter().flatten() {
        out.push(slot_json(slot));
    }
    out
}

fn note_json(n: &crate::model::Note) -> Value {
    let mut o = json!({
        "pitch": n.pitch,
        "start": round6(f64::from(n.start)),
        "length": round6(f64::from(n.len)),
        "velocity": n.vel,
    });
    if let Some(m) = o.as_object_mut() {
        if n.mute {
            m.insert("mute".into(), true.into());
        }
        if !n.lyric.is_empty() {
            m.insert("lyric".into(), n.lyric.as_str().into());
        }
    }
    o
}

pub fn region_kind(r: &Region) -> &'static str {
    if r.is_audio() {
        "audio"
    } else {
        match r.generated.as_ref().map(|g| &g.source) {
            Some(crate::model::Generator::Pattern(_)) => "pattern",
            Some(crate::model::Generator::Drummer(_)) => "drummer",
            None => "midi",
        }
    }
}

pub fn region_json(p: &Project, ids: &Ids, ti: usize, ri: usize, notes: bool) -> Value {
    let Some(r) = p.tracks.get(ti).and_then(|t| t.regions.get(ri)) else {
        return Value::Null;
    };
    let start = p.region_start_beats(r);
    let len = p.region_len_beats(r);
    let mut o = Map::new();
    o.insert("id".into(), ids.region_id(ti, ri).into());
    o.insert("track".into(), ids.track_id(ti).into());
    o.insert("name".into(), r.name.clone().into());
    o.insert("kind".into(), region_kind(r).into());
    o.insert("start_bar".into(), json!(round6(f64::from(r.start_bar))));
    o.insert("length_bars".into(), json!(round6(f64::from(r.len_bars))));
    o.insert("start_beat".into(), json!(round6(start)));
    o.insert("length_beats".into(), json!(round6(len)));
    o.insert(
        "start_seconds".into(),
        json!(round6(p.tempo_map().seconds_at(start))),
    );
    o.insert("mute".into(), r.params.mute.into());
    o.insert(
        "loop_beats".into(),
        json!(round6(f64::from(r.params.loop_beats))),
    );
    if r.is_audio() {
        let mut a = Map::new();
        if let Some(audio) = &r.audio {
            a.insert("file".into(), audio.file.path.display().to_string().into());
            a.insert(
                "file_seconds".into(),
                json!(round6(audio.file.duration_secs())),
            );
            a.insert("sample_rate".into(), audio.file.sample_rate.into());
            a.insert("channels".into(), audio.file.channels.into());
            a.insert("flex".into(), audio.flex.is_some().into());
        }
        if let Some(m) = &r.missing {
            a.insert("missing_file".into(), m.source.display().to_string().into());
        }
        a.insert("offset_seconds".into(), json!(round6(r.params.offset_secs)));
        a.insert("gain_db".into(), json!(round3(f64::from(r.params.gain_db))));
        a.insert(
            "fade_in_beats".into(),
            json!(round6(f64::from(r.params.fade_in))),
        );
        a.insert(
            "fade_out_beats".into(),
            json!(round6(f64::from(r.params.fade_out))),
        );
        a.insert("reverse".into(), r.params.reverse.into());
        o.insert("audio".into(), Value::Object(a));
    } else {
        o.insert("transpose".into(), r.params.transpose.into());
        o.insert("velocity_offset".into(), r.params.velocity.into());
        o.insert("quantize_beats".into(), json!(r.params.quantize));
        o.insert("note_count".into(), r.notes.len().into());
        o.insert("take_folder".into(), r.folder.is_some().into());
        if notes {
            o.insert(
                "notes".into(),
                Value::Array(r.notes.iter().map(note_json).collect()),
            );
            if !r.controls.is_empty() {
                o.insert("controller_events".into(), r.controls.len().into());
            }
        }
    }
    Value::Object(o)
}

fn instrument_json(t: &Track) -> Value {
    if let Some(slot) = &t.au_instrument {
        return slot_json(slot);
    }
    if let Some(sf) = &t.soundfont {
        return json!({
            "id": "soundfont",
            "name": sf.path,
            "format": "soundfont",
            "bank": sf.bank,
            "program": sf.program,
        });
    }
    Value::Null
}

pub fn track_json(p: &Project, ids: &Ids, ti: usize, regions: bool, notes: bool) -> Value {
    let Some(t) = p.tracks.get(ti) else {
        return Value::Null;
    };
    let mut o = Map::new();
    o.insert("id".into(), ids.track_id(ti).into());
    o.insert("number".into(), (ti + 1).into());
    o.insert("name".into(), t.name.clone().into());
    o.insert("kind".into(), kind_name(t.kind).into());
    o.insert("selected".into(), (p.selected_track == ti).into());
    o.insert("mute".into(), t.mute.into());
    o.insert("solo".into(), t.solo.into());
    o.insert("arm".into(), t.rec.into());
    o.insert("monitor".into(), t.monitor.into());
    o.insert("volume_db".into(), json!(round3(f64::from(t.volume_db))));
    o.insert("pan".into(), json!(round3(f64::from(t.pan))));
    o.insert("color".into(), color_hex(t.color).into());
    o.insert(
        "output".into(),
        if t.output == 0 {
            "stereo_out".into()
        } else {
            format!("bus:{}", t.output).into()
        },
    );
    if t.kind == TrackKind::Audio {
        o.insert("input".into(), input_name(t.input).into());
    }
    if matches!(t.kind, TrackKind::Instrument | TrackKind::Drummer) {
        o.insert("instrument".into(), instrument_json(t));
    }
    if t.kind == TrackKind::ExternalMidi {
        o.insert("midi_port".into(), json!(t.midi_port));
        o.insert("midi_channel".into(), t.midi_channel.into());
    }
    o.insert("effects".into(), Value::Array(effects_json(p, ti)));
    let sends: Vec<Value> = t
        .sends
        .iter()
        .enumerate()
        .filter_map(|(k, s)| {
            s.bus.map(|bus| {
                json!({"slot": k + 1, "bus": bus, "level": round3(f64::from(s.level)), "pre_fader": s.pre})
            })
        })
        .collect();
    o.insert("sends".into(), Value::Array(sends));
    o.insert("indent".into(), t.indent.into());
    if let Some(v) = t.vca {
        o.insert("vca".into(), ids.track_id(v).into());
    }
    if let Some(g) = t.group.and_then(|g| p.groups.get(g)) {
        o.insert("group".into(), g.name.clone().into());
    }
    let lanes: Vec<Value> = t
        .auto
        .lanes()
        .filter(|(_, pts)| !pts.is_empty())
        .map(|(target, pts)| json!({"param": crate::automation::target_key(target), "points": pts.len()}))
        .collect();
    o.insert("automation".into(), Value::Array(lanes));
    if regions {
        let list: Vec<Value> = (0..t.regions.len())
            .map(|ri| region_json(p, ids, ti, ri, notes))
            .collect();
        o.insert("regions".into(), Value::Array(list));
    } else {
        o.insert("region_count".into(), t.regions.len().into());
    }
    Value::Object(o)
}

pub fn input_name(input: prowave_engine::InputSource) -> String {
    match input {
        prowave_engine::InputSource::Mono(c) => format!("mono:{}", c + 1),
        prowave_engine::InputSource::Stereo(c) => format!("stereo:{}", c + 1),
    }
}

pub fn transport_json(p: &Project) -> Value {
    json!({
        "playing": p.playing,
        "recording": p.recording,
        "counting_in": p.counting_in,
        "playhead": place_json(p, p.position_beats),
        "tempo": round3(f64::from(p.tempo_at(p.position_beats))),
        "signature": {
            "numerator": p.sigs().signature_of_bar(p.beats_to_bar(p.position_beats) as u32).0,
            "denominator": p.sigs().signature_of_bar(p.beats_to_bar(p.position_beats) as u32).1,
        },
        "cycle": {
            "on": p.cycle,
            "start": place_json(p, p.cycle_start),
            "end": place_json(p, p.cycle_end),
        },
        "metronome": p.metronome,
        "count_in": {"on": p.count_in, "bars": p.count_in_bars},
    })
}

pub fn markers_json(p: &Project) -> Value {
    p.markers
        .iter()
        .enumerate()
        .map(|(i, m)| {
            json!({
                "number": i + 1,
                "name": m.name,
                "at": place_json(p, m.beat),
                "color": color_hex(m.color),
            })
        })
        .collect()
}

pub fn tempo_json(p: &Project) -> Value {
    p.tempo_points()
        .iter()
        .map(|t| json!({"at": place_json(p, t.beat), "bpm": round3(f64::from(t.bpm)), "ramp": t.ramp}))
        .collect()
}

pub fn signatures_json(p: &Project) -> Value {
    let mut out = vec![json!({"bar": 1, "numerator": p.sig.0, "denominator": p.sig.1})];
    out.extend(
        p.sig_changes
            .iter()
            .map(|c| json!({"bar": c.bar, "numerator": c.num, "denominator": c.den})),
    );
    Value::Array(out)
}

pub fn file_json(app: &App) -> Value {
    json!({
        "name": app.project.name,
        "path": app.path.as_ref().map(|p| p.display().to_string()),
        "dirty": app.has_unsaved_changes(),
        "can_undo": app.history.can_undo(),
        "can_redo": app.history.can_redo(),
    })
}

/// The whole project.
pub fn project_json(app: &App, ids: &Ids, regions: bool, notes: bool) -> Value {
    let p = &app.project;
    let tracks: Vec<Value> = (0..p.tracks.len())
        .map(|ti| track_json(p, ids, ti, regions, notes))
        .collect();
    json!({
        "project": file_json(app),
        "transport": transport_json(p),
        "key": p.key,
        "tuning_hz": round3(f64::from(p.tuning_hz)),
        "tempo_changes": tempo_json(p),
        "signatures": signatures_json(p),
        "markers": markers_json(p),
        "tracks": tracks,
        "stereo_out": {
            "id": "master",
            "volume_db": round3(f64::from(crate::mixer::lin_to_db(p.master_vol))),
            "effects": effects_json(p, MASTER_STRIP),
        },
        "selection": {
            "track": ids.track_id(p.selected_track),
            "regions": app.ui.tracks.selection().into_iter().map(|(t, r)| ids.region_id(t, r)).collect::<Vec<_>>(),
        },
        "busy": {
            "fit_to_grid": app.ui.fit.busy(),
            "recording_midi": app.ui.rec.active(),
            "recording_audio": app.ui.audio_rec.active(),
        },
        "status": app.ui.status,
        "dialog": super::m_ui::open_dialog(app).map(|d| d.to_json()),
    })
}
